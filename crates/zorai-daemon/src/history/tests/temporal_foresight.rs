use super::*;
use crate::history::schema_helpers::table_has_column;
use std::fs;

#[tokio::test]
async fn init_schema_adds_temporal_foresight_and_intent_model_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS precomputation_log;
                DROP TABLE IF EXISTS temporal_predictions;
                DROP TABLE IF EXISTS temporal_patterns;
                DROP TABLE IF EXISTS intent_models;
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
            let has_pattern_type = table_has_column(conn, "temporal_patterns", "pattern_type")?;
            let has_timescale = table_has_column(conn, "temporal_patterns", "timescale")?;
            let pattern_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_temporal_patterns_type_scale'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let has_confidence = table_has_column(conn, "temporal_predictions", "confidence")?;
            let prediction_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_temporal_predictions_pattern_predicted'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let has_precompute_type =
                table_has_column(conn, "precomputation_log", "precomputation_type")?;
            let has_model_blob = table_has_column(conn, "intent_models", "model_blob")?;
            Ok((
                has_pattern_type,
                has_timescale,
                pattern_index,
                has_confidence,
                prediction_index,
                has_precompute_type,
                has_model_blob,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert_eq!(
        status.2.as_deref(),
        Some("idx_temporal_patterns_type_scale")
    );
    assert!(status.3);
    assert_eq!(
        status.4.as_deref(),
        Some("idx_temporal_predictions_pattern_predicted")
    );
    assert!(status.5);
    assert!(status.6);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn temporal_foresight_round_trips_patterns_predictions_precomputations_and_models(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let pattern_id = store
        .insert_temporal_pattern(&TemporalPatternRow {
            id: None,
            pattern_type: "task_sequence".to_string(),
            timescale: "minutes".to_string(),
            pattern_description: "After editing Rust, cargo check follows".to_string(),
            context_filter: Some("project=cmux-next".to_string()),
            frequency: 4,
            last_observed_ms: 160,
            first_observed_ms: 100,
            confidence: 0.83,
            decay_rate: 0.01,
            created_at_ms: 100,
        })
        .await?;
    let prediction_id = store
        .insert_temporal_prediction(&TemporalPredictionRow {
            id: None,
            pattern_id,
            predicted_action: "cargo check".to_string(),
            predicted_at_ms: 170,
            confidence: 0.83,
            actual_action: None,
            was_accepted: None,
            accuracy_score: None,
        })
        .await?;
    store
        .insert_precomputation_log(&PrecomputationLogRow {
            id: None,
            prediction_id,
            precomputation_type: "context_prefetch".to_string(),
            precomputation_details: "prefetch repo status".to_string(),
            started_at_ms: 171,
            completed_at_ms: Some(175),
            was_used: Some(true),
        })
        .await?;
    store
        .upsert_intent_model(&IntentModelRow {
            id: None,
            agent_id: "weles".to_string(),
            model_blob: Some(vec![1, 2, 3, 4]),
            created_at_ms: 180,
            accuracy_score: Some(0.71),
        })
        .await?;

    let patterns = store.list_temporal_patterns("task_sequence", 4).await?;
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].id, Some(pattern_id));
    assert_eq!(patterns[0].timescale, "minutes");

    let predictions = store.list_temporal_predictions(pattern_id, 4).await?;
    assert_eq!(predictions.len(), 1);
    assert_eq!(predictions[0].id, Some(prediction_id));
    assert_eq!(predictions[0].predicted_action, "cargo check");

    let precomputations = store.list_precomputation_log(prediction_id).await?;
    assert_eq!(precomputations.len(), 1);
    assert_eq!(precomputations[0].precomputation_type, "context_prefetch");
    assert_eq!(precomputations[0].was_used, Some(true));

    let model = store
        .get_intent_model("weles")
        .await?
        .expect("intent model should load");
    assert_eq!(model.agent_id, "weles");
    assert_eq!(model.accuracy_score, Some(0.71));

    fs::remove_dir_all(root)?;
    Ok(())
}
