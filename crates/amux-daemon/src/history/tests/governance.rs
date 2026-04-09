use super::*;
use serde_json::json;

#[tokio::test]
async fn init_schema_adds_governance_tables_to_legacy_db() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS approval_records;
                DROP TABLE IF EXISTS governance_evaluations;
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
            let has_approval_transition =
                table_has_column(conn, "approval_records", "transition_kind")?;
            let has_approval_policy =
                table_has_column(conn, "approval_records", "policy_fingerprint")?;
            let has_eval_transition =
                table_has_column(conn, "governance_evaluations", "transition_kind")?;
            let has_eval_verdict =
                table_has_column(conn, "governance_evaluations", "verdict_json")?;
            let approval_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_approval_records_policy'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let eval_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_governance_evaluations_policy'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((
                has_approval_transition,
                has_approval_policy,
                has_eval_transition,
                has_eval_verdict,
                approval_index,
                eval_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert!(status.2);
    assert!(status.3);
    assert_eq!(status.4.as_deref(), Some("idx_approval_records_policy"));
    assert_eq!(
        status.5.as_deref(),
        Some("idx_governance_evaluations_policy")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn approval_record_round_trips_and_updates() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let approval = ApprovalRecordRow {
        approval_id: "approval-1".to_string(),
        run_id: Some("run-1".to_string()),
        task_id: Some("task-1".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        thread_id: Some("thread-1".to_string()),
        transition_kind: "managed_command_dispatch".to_string(),
        stage_id: Some("stage-1".to_string()),
        scope_summary: Some("dispatch to current workspace".to_string()),
        target_scope_json: json!(["workspace-a", "lane-1"]).to_string(),
        constraints_json: json!([
            {"kind": "serial_only_execution", "value": null, "rationale": "one lane only"}
        ])
        .to_string(),
        risk_class: "high".to_string(),
        rationale_json: json!(["network side effects without sandboxing"]).to_string(),
        policy_fingerprint: "fingerprint-1".to_string(),
        requested_at: 100,
        resolved_at: None,
        expires_at: Some(400),
        resolution: None,
        invalidated_at: None,
        invalidation_reason: None,
    };

    store.insert_approval_record(&approval).await?;
    let loaded = store
        .get_approval_record("approval-1")
        .await?
        .expect("approval record should exist");
    assert_eq!(loaded, approval);

    store
        .resolve_approval_record("approval-1", "approved", 150)
        .await?;
    let resolved = store
        .get_approval_record("approval-1")
        .await?
        .expect("approval record should still exist");
    assert_eq!(resolved.resolution.as_deref(), Some("approved"));
    assert_eq!(resolved.resolved_at, Some(150));

    store
        .invalidate_approval_record("approval-1", "policy changed", 175)
        .await?;
    let invalidated = store
        .get_approval_record("approval-1")
        .await?
        .expect("approval record should still exist");
    assert_eq!(
        invalidated.invalidation_reason.as_deref(),
        Some("policy changed")
    );
    assert_eq!(invalidated.invalidated_at, Some(175));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn governance_evaluation_persists_expected_row() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let evaluation = GovernanceEvaluationRow {
        id: "eval-1".to_string(),
        run_id: Some("run-1".to_string()),
        task_id: Some("task-1".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        thread_id: Some("thread-1".to_string()),
        transition_kind: "managed_command_dispatch".to_string(),
        input_json: json!({
            "requested_action_summary": "cargo test",
            "target_ids": ["workspace-a"]
        })
        .to_string(),
        verdict_json: json!({
            "verdict_class": "allow_with_constraints",
            "risk_class": "medium"
        })
        .to_string(),
        policy_fingerprint: "fingerprint-2".to_string(),
        created_at: 222,
    };

    store.insert_governance_evaluation(&evaluation).await?;

    let loaded = store
        .conn
        .call(|conn| {
            conn.query_row(
                "SELECT id, run_id, task_id, goal_run_id, thread_id, transition_kind, input_json, verdict_json, policy_fingerprint, created_at FROM governance_evaluations WHERE id = ?1",
                params!["eval-1"],
                |row| {
                    Ok(GovernanceEvaluationRow {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        task_id: row.get(2)?,
                        goal_run_id: row.get(3)?,
                        thread_id: row.get(4)?,
                        transition_kind: row.get(5)?,
                        input_json: row.get(6)?,
                        verdict_json: row.get(7)?,
                        policy_fingerprint: row.get(8)?,
                        created_at: row.get::<_, i64>(9)? as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .expect("governance evaluation should exist");

    assert_eq!(loaded, evaluation);

    fs::remove_dir_all(root)?;
    Ok(())
}
