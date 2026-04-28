use crate::agent::harness::{
    build_harness_state_payload, load_harness_state_projection, run_minimal_closed_loop,
    EffectExecutionKind, HarnessLoopInput,
};
use crate::history::HistoryStore;
use anyhow::Result;
use tempfile::tempdir;

#[tokio::test]
async fn harness_projection_rebuilds_current_state() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    let result = run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-harness".to_string()),
            goal_run_id: Some("goal-harness".to_string()),
            task_id: Some("task-harness".to_string()),
            observation_summary: "workspace drift detected".to_string(),
            observation_details: serde_json::json!({
                "source": "test",
                "state": {"actual": "drifted"},
            }),
            goal_summary: "stabilize harness loop".to_string(),
            desired_state: None,
            preferred_effect_kind: Some(EffectExecutionKind::Mutating),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    assert_eq!(result.projection.observations.len(), 1);
    assert_eq!(result.projection.beliefs.len(), 2);
    assert_eq!(result.projection.goals.len(), 1);
    assert_eq!(result.projection.world_states.len(), 1);
    assert!(!result.projection.tensions.is_empty());
    assert!(!result.projection.commitments.is_empty());
    assert_eq!(result.projection.effects.len(), 1);
    assert_eq!(result.projection.verifications.len(), 1);
    assert_eq!(result.projection.procedures.len(), 1);
    assert_eq!(result.projection.effect_contracts.len(), 1);
    assert_eq!(result.world_state_id, result.projection.world_states[0].id);
    assert_eq!(
        result.effect_contract_id,
        result.projection.effect_contracts[0].id
    );
    assert!(result.projection.verifications[0].verified);
    assert_eq!(result.procedure_id, result.projection.procedures[0].id);
    assert_eq!(result.projection.procedures[0].successful_trace_count, 1);
    assert_eq!(result.projection.procedures[0].failed_trace_count, 0);
    assert_eq!(
        result.projection.procedures[0].status,
        crate::agent::harness::ProcedureStatus::Candidate
    );
    assert!(result.projection.procedures[0].verified_outcome);

    Ok(())
}

#[tokio::test]
async fn harness_mvp_loop_runs_observation_to_verified_effect() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    let result = run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-loop".to_string()),
            goal_run_id: Some("goal-loop".to_string()),
            task_id: Some("task-loop".to_string()),
            observation_summary: "new observation arrives with contradiction".to_string(),
            observation_details: serde_json::json!({
                "kind": "integration",
                "state": {"actual": "out_of_sync"},
                "contradictions": ["actual state differs from intended state"],
            }),
            goal_summary: "prove the minimal closed loop".to_string(),
            desired_state: None,
            preferred_effect_kind: Some(EffectExecutionKind::Mutating),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let projection = load_harness_state_projection(
        &store,
        Some("thread-loop"),
        Some("goal-loop"),
        Some("task-loop"),
    )
    .await?;
    assert_eq!(projection.world_states.len(), 1);
    assert_eq!(projection.effects.len(), 1);
    assert_eq!(projection.verifications.len(), 1);
    assert!(projection.effects[0].dispatch_success);
    assert!(projection.verifications[0].verified);
    assert!(projection.effect_contracts[0]
        .verification_gates
        .iter()
        .all(|gate| gate.name != "desired_state_match"));
    assert!(!result.selected_tension_ids.is_empty());
    assert_eq!(result.effect_id, projection.effects[0].id);
    assert_eq!(result.verification_id, projection.verifications[0].id);

    let events = store
        .list_event_log(Some("harness"), Some("loop_completed"), 8)
        .await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].state.as_deref(), Some("verified"));
    assert_eq!(
        projection.procedures[0].preferred_effect_order[0],
        "mutating"
    );

    Ok(())
}

#[tokio::test]
async fn harness_event_log_captures_effect_and_verification() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    let result = run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-event".to_string()),
            goal_run_id: Some("goal-event".to_string()),
            task_id: Some("task-event".to_string()),
            observation_summary: "emit operator-visible trace with blocker".to_string(),
            observation_details: serde_json::json!({
                "unknowns": ["missing exact file to patch"],
                "state": {"actual": "unknown"},
            }),
            goal_summary: "persist trace".to_string(),
            desired_state: None,
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let events = store
        .list_event_log(Some("harness"), Some("loop_completed"), 8)
        .await?;
    assert_eq!(events.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&events[0].payload_json)?;
    assert_eq!(payload["world_state_id"], result.world_state_id);
    assert_eq!(payload["effect_contract_id"], result.effect_contract_id);
    assert_eq!(payload["effect_id"], result.effect_id);
    assert_eq!(payload["verification_id"], result.verification_id);
    assert_eq!(payload["procedure_id"], result.procedure_id);
    assert!(payload["tension_ids"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));

    let report = store.provenance_report(8)?;
    assert!(report
        .entries
        .iter()
        .any(|entry| entry.event_type == "harness_loop_completed"));

    Ok(())
}

#[tokio::test]
async fn harness_read_only_risk_escalation_stays_blocked_by_governance() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    let result = run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-risk".to_string()),
            goal_run_id: Some("goal-risk".to_string()),
            task_id: Some("task-risk".to_string()),
            observation_summary: "approval-sensitive external observation required".to_string(),
            observation_details: serde_json::json!({
                "risk_flags": ["approval_required", "external_surface"],
                "state": {"actual": "needs_remote_inspection"},
            }),
            goal_summary: "exercise governance gates".to_string(),
            desired_state: None,
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: true,
            sandbox_enabled: false,
        },
    )
    .await?;

    let projection = load_harness_state_projection(
        &store,
        Some("thread-risk"),
        Some("goal-risk"),
        Some("task-risk"),
    )
    .await?;

    assert_eq!(projection.effects.len(), 1);
    assert_eq!(projection.verifications.len(), 1);
    assert!(!projection.effects[0].dispatch_success);
    assert!(!projection.verifications[0].verified);
    assert_eq!(
        projection.commitments.last().map(|c| &c.status),
        Some(&crate::agent::harness::CommitmentStatus::Blocked)
    );
    assert_eq!(projection.procedures[0].failed_trace_count, 1);
    assert!(!projection.procedures[0].verified_outcome);
    assert_eq!(
        projection.procedures[0]
            .preferred_effect_order
            .last()
            .map(String::as_str),
        Some("read_only")
    );
    assert!(result
        .selected_tension_ids
        .iter()
        .any(|id| projection.tensions.iter().any(|tension| tension.id == *id)));

    Ok(())
}

#[tokio::test]
async fn harness_selects_highest_priority_tensions_and_persists_commitment_critique() -> Result<()>
{
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    let result = run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-priority".to_string()),
            goal_run_id: Some("goal-priority".to_string()),
            task_id: Some("task-priority".to_string()),
            observation_summary: "conflicting governed follow-up signals".to_string(),
            observation_details: serde_json::json!({
                "risk_flags": ["approval_required"],
                "contradictions": ["workspace state conflicts with intended correction"],
                "unknowns": ["exact safe mutation path still unknown"],
                "state": {"actual": "conflicted"},
            }),
            goal_summary: "rank tensions before execution".to_string(),
            desired_state: None,
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let projection = load_harness_state_projection(
        &store,
        Some("thread-priority"),
        Some("goal-priority"),
        Some("task-priority"),
    )
    .await?;

    let max_priority = projection
        .tensions
        .iter()
        .map(|tension| tension.priority)
        .max()
        .unwrap_or(0);
    let selected_tensions: Vec<_> = projection
        .tensions
        .iter()
        .filter(|tension| result.selected_tension_ids.contains(&tension.id))
        .collect();

    assert!(!selected_tensions.is_empty());
    assert!(selected_tensions
        .iter()
        .all(|tension| tension.priority == max_priority));

    let commitment = projection.commitments.last().expect("commitment persisted");
    assert_eq!(
        commitment.critique_summary.as_deref(),
        Some("A read-only commitment is acceptable only if it sharpens the next governed move")
    );
    assert_eq!(commitment.role_assessments.len(), 5);
    assert_eq!(
        commitment.source_world_state_id.as_deref(),
        Some(result.world_state_id.as_str())
    );

    Ok(())
}

#[tokio::test]
async fn harness_desired_state_gate_reflects_subset_match() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let desired_state = serde_json::json!({
        "actual": "aligned",
    });

    run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-desired-pass".to_string()),
            goal_run_id: Some("goal-desired-pass".to_string()),
            task_id: Some("task-desired-pass".to_string()),
            observation_summary: "desired state is already satisfied".to_string(),
            observation_details: serde_json::json!({
                "state": {"actual": "aligned"},
            }),
            goal_summary: "prove desired-state verification passes on subset match".to_string(),
            desired_state: Some(desired_state.clone()),
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let passing_projection = load_harness_state_projection(
        &store,
        Some("thread-desired-pass"),
        Some("goal-desired-pass"),
        Some("task-desired-pass"),
    )
    .await?;
    assert!(passing_projection.effect_contracts[0]
        .verification_gates
        .iter()
        .any(|gate| gate.name == "desired_state_match"));
    assert!(passing_projection.verifications[0].verified);

    let passing_gate = passing_projection.verifications[0].details["gate_results"]
        .as_array()
        .and_then(|gates| {
            gates
                .iter()
                .find(|gate| gate["gate_name"] == "desired_state_match")
        })
        .expect("desired_state_match gate result persisted for passing case");
    assert_eq!(passing_gate["passed"], serde_json::json!(true));

    run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-desired-fail".to_string()),
            goal_run_id: Some("goal-desired-fail".to_string()),
            task_id: Some("task-desired-fail".to_string()),
            observation_summary: "desired state remains unsatisfied".to_string(),
            observation_details: serde_json::json!({
                "state": {"actual": "drifted"},
            }),
            goal_summary: "prove desired-state verification fails on mismatch".to_string(),
            desired_state: Some(desired_state),
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let failing_projection = load_harness_state_projection(
        &store,
        Some("thread-desired-fail"),
        Some("goal-desired-fail"),
        Some("task-desired-fail"),
    )
    .await?;
    assert!(failing_projection.effect_contracts[0]
        .verification_gates
        .iter()
        .any(|gate| gate.name == "desired_state_match"));
    assert!(!failing_projection.verifications[0].verified);

    let failing_gate = failing_projection.verifications[0].details["gate_results"]
        .as_array()
        .and_then(|gates| {
            gates
                .iter()
                .find(|gate| gate["gate_name"] == "desired_state_match")
        })
        .expect("desired_state_match gate result persisted for failing case");
    assert_eq!(failing_gate["passed"], serde_json::json!(false));

    Ok(())
}

#[tokio::test]
async fn harness_procedure_distills_repeated_successful_trace() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    for _ in 0..2 {
        run_minimal_closed_loop(
            &store,
            HarnessLoopInput {
                thread_id: Some("thread-distill".to_string()),
                goal_run_id: Some("goal-distill".to_string()),
                task_id: Some("task-distill".to_string()),
                observation_summary: "repeat the same successful governed trace".to_string(),
                observation_details: serde_json::json!({
                    "state": {"actual": "aligned"},
                    "opportunities": ["Preserve the successful governed loop"],
                }),
                goal_summary: "distill a reusable governed procedure".to_string(),
                desired_state: Some(serde_json::json!({"actual": "aligned"})),
                preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
                allow_network: false,
                sandbox_enabled: true,
            },
        )
        .await?;
    }

    let projection = load_harness_state_projection(
        &store,
        Some("thread-distill"),
        Some("goal-distill"),
        Some("task-distill"),
    )
    .await?;

    assert_eq!(projection.procedures.len(), 2);
    let latest = projection.procedures.last().expect("latest procedure");
    assert_eq!(
        latest.status,
        crate::agent::harness::ProcedureStatus::Learned
    );
    assert!(latest.verified_outcome);
    assert_eq!(latest.successful_trace_count, 2);
    assert_eq!(latest.failed_trace_count, 0);
    assert!(
        latest.confidence > 0.9,
        "confidence should rise after repeated success"
    );
    assert_eq!(
        latest.preferred_effect_order.first().map(String::as_str),
        Some("read_only")
    );
    assert_eq!(
        latest.source_verification_id.as_deref(),
        latest.details["verification_id"].as_str()
    );

    Ok(())
}

#[tokio::test]
async fn harness_state_payload_surfaces_persisted_visibility_sections() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;

    run_minimal_closed_loop(
        &store,
        HarnessLoopInput {
            thread_id: Some("thread-surface".to_string()),
            goal_run_id: Some("goal-surface".to_string()),
            task_id: Some("task-surface".to_string()),
            observation_summary: "surface persisted harness state".to_string(),
            observation_details: serde_json::json!({
                "state": {"actual": "aligned"},
                "unknowns": ["operator still needs an inspectable view"],
                "opportunities": ["Ship a developer-facing visibility surface"],
            }),
            goal_summary: "render the harness visibility surface".to_string(),
            desired_state: Some(serde_json::json!({"actual": "aligned"})),
            preferred_effect_kind: Some(EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await?;

    let projection = load_harness_state_projection(
        &store,
        Some("thread-surface"),
        Some("goal-surface"),
        Some("task-surface"),
    )
    .await?;
    let payload = build_harness_state_payload(
        &projection,
        Some("thread-surface"),
        Some("goal-surface"),
        Some("task-surface"),
        3,
    );

    assert_eq!(payload["scope"]["thread_id"], "thread-surface");
    assert_eq!(payload["counts"]["procedures"], serde_json::json!(1));
    assert!(payload["world_state"]["latest"].is_object());
    assert!(payload["tensions"]["active"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    assert!(payload["commitments"]["latest"].is_object());
    assert!(payload["effects"]["latest"].is_object());
    assert!(payload["verifications"]["latest"].is_object());
    assert!(payload["procedures"]["latest"].is_object());
    assert_eq!(
        payload["procedures"]["latest"]["source_verification_id"],
        projection
            .procedures
            .last()
            .and_then(|record| record.source_verification_id.clone())
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null)
    );

    Ok(())
}
