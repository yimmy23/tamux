use super::*;
use crate::agent::harness::{
    placeholder_governance_input, EffectContractRecord, EffectExecutionKind, HarnessRecordEnvelope,
    HarnessRecordKind, ObservationKind, ObservationRecord, VerificationGateKind,
    VerificationGateRecord,
};
use crate::history::schema_helpers::table_has_column;
use std::fs;

fn scope_ids() -> (String, String, String) {
    (
        "thread-harness-history".to_string(),
        "goal-harness-history".to_string(),
        "task-harness-history".to_string(),
    )
}

#[tokio::test]
async fn init_schema_adds_harness_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let status = store
        .conn
        .call(|conn| {
            Ok((
                table_has_column(conn, "harness_state_records", "entity_id")?,
                table_has_column(conn, "harness_state_records", "payload_json")?,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn harness_records_round_trip_via_history_store() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let (thread_id, goal_run_id, task_id) = scope_ids();

    let observation = ObservationRecord {
        id: "obs-history-1".to_string(),
        thread_id: Some(thread_id.clone()),
        goal_run_id: Some(goal_run_id.clone()),
        task_id: Some(task_id.clone()),
        kind: ObservationKind::SystemSignal,
        summary: "history layer round trip".to_string(),
        details: serde_json::json!({"source": "history_test"}),
        created_at_ms: 1,
    };
    let contract = EffectContractRecord {
        id: "contract-history-1".to_string(),
        thread_id: Some(thread_id.clone()),
        goal_run_id: Some(goal_run_id.clone()),
        task_id: Some(task_id.clone()),
        summary: "history contract".to_string(),
        execution_kind: EffectExecutionKind::ReadOnly,
        reversible: true,
        risk_hint: "low".to_string(),
        blast_radius_hint: "goal-local inspection only".to_string(),
        preconditions: vec!["projection exists".to_string()],
        expected_effects: vec!["record persisted".to_string()],
        verification_strategy: "payload persisted".to_string(),
        verification_gates: vec![VerificationGateRecord {
            name: "effect_persisted".to_string(),
            kind: VerificationGateKind::EffectPersisted,
            description: "persisted row exists".to_string(),
            required: true,
            evidence_key: Some("effect_id".to_string()),
        }],
        governance_input: placeholder_governance_input(
            Some(&thread_id),
            Some(&goal_run_id),
            Some(&task_id),
            "history harness contract",
        ),
        created_at_ms: 2,
    };

    store
        .append_harness_state_record(&HarnessRecordEnvelope {
            entry_id: "entry-obs-history-1".to_string(),
            entity_id: observation.id.clone(),
            thread_id: observation.thread_id.clone(),
            goal_run_id: observation.goal_run_id.clone(),
            task_id: observation.task_id.clone(),
            kind: HarnessRecordKind::Observation,
            status: None,
            summary: observation.summary.clone(),
            payload: serde_json::to_value(&observation)?,
            created_at_ms: observation.created_at_ms,
        })
        .await?;
    store
        .append_harness_state_record(&HarnessRecordEnvelope {
            entry_id: "entry-contract-history-1".to_string(),
            entity_id: contract.id.clone(),
            thread_id: contract.thread_id.clone(),
            goal_run_id: contract.goal_run_id.clone(),
            task_id: contract.task_id.clone(),
            kind: HarnessRecordKind::EffectContract,
            status: None,
            summary: contract.summary.clone(),
            payload: serde_json::to_value(&contract)?,
            created_at_ms: contract.created_at_ms,
        })
        .await?;

    let state = store
        .project_harness_state(Some(&thread_id), Some(&goal_run_id), Some(&task_id))
        .await?;
    let rows = store
        .list_harness_state_records(Some(&thread_id), Some(&goal_run_id), Some(&task_id))
        .await?;

    assert_eq!(state.observations.len(), 1);
    assert_eq!(state.effect_contracts.len(), 1);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| !row.entry_id.is_empty()));
    assert!(rows
        .iter()
        .any(|row| row.kind == HarnessRecordKind::EffectContract));

    fs::remove_dir_all(root)?;
    Ok(())
}
