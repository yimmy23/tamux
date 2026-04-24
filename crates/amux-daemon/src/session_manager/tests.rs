use super::*;

#[cfg(unix)]
#[tokio::test]
async fn list_omits_dead_sessions_and_managed_execution_rejects_them() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(Some("/bin/true".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(
        manager
            .list()
            .await
            .into_iter()
            .all(|session| session.id != session_id),
        "dead sessions should not be offered as active choices"
    );

    let error = manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "echo hello".to_string(),
                rationale: "test".to_string(),
                allow_network: false,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Lowest,
                cwd: None,
                language_hint: None,
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect_err("dead sessions must be rejected for managed execution");

    assert!(error.to_string().contains("not alive"));
}

#[cfg(unix)]
#[tokio::test]
async fn managed_command_governance_persists_evaluation_and_approval() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(
            Some("/bin/sh".to_string()),
            None,
            Some("workspace-a".to_string()),
            None,
            80,
            24,
        )
        .await
        .expect("spawn test session");

    let message = manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return a daemon message");

    let approval_id = match message {
        DaemonMessage::ApprovalRequired { approval, .. } => {
            assert_eq!(approval.risk_level, "high");
            assert_eq!(approval.workspace_id.as_deref(), Some("workspace-a"));
            assert_eq!(
                approval.transition_kind.as_deref(),
                Some("managed_command_dispatch")
            );
            assert!(approval
                .policy_fingerprint
                .as_deref()
                .is_some_and(|fp| fp.len() > 8));
            assert!(approval.expires_at.is_some());
            assert!(!approval.constraints.is_empty());
            assert!(approval.scope_summary.is_some());
            approval.approval_id
        }
        other => panic!("expected approval required, got {other:?}"),
    };

    let approval = manager
        .history
        .get_approval_record(&approval_id)
        .await
        .expect("approval lookup should succeed")
        .expect("approval record should exist");
    assert_eq!(approval.transition_kind, "managed_command_dispatch");
    assert_eq!(approval.stage_id.as_deref(), Some("managed_dispatch"));
    assert_eq!(approval.risk_class, "high");
    assert!(approval.scope_summary.is_some());
    assert!(approval.policy_fingerprint.len() > 8);
    assert!(approval.target_scope_json.contains("workspace-a"));
    assert!(approval.target_scope_json.contains(&session_id.to_string()));

    let evaluation = manager
        .history
        .conn
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT input_json FROM governance_evaluations ORDER BY created_at DESC LIMIT 1",
            )?;
            let input_json: String = stmt.query_row([], |row| row.get(0))?;
            Ok(input_json)
        })
        .await
        .expect("evaluation lookup should succeed");
    let evaluation: serde_json::Value =
        serde_json::from_str(&evaluation).expect("evaluation input json should parse");
    let run_id = evaluation["run_id"]
        .as_str()
        .expect("run_id should be a string");
    assert!(run_id.starts_with("exec_"));
    assert_eq!(evaluation["stage_id"], "managed_dispatch");
    assert_eq!(
        evaluation["lane_ids"],
        serde_json::json!([session_id.to_string()])
    );
    assert_eq!(evaluation["target_ids"], serde_json::json!(["workspace-a"]));

    let eval_count: i64 = manager
        .history
        .conn
        .call(|conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM governance_evaluations", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .await
        .expect("evaluation count query should succeed");
    assert_eq!(eval_count, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn managed_command_governance_persists_causal_trace_and_audit_for_risky_transition() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(
            Some("/bin/sh".to_string()),
            None,
            Some("workspace-a".to_string()),
            None,
            80,
            24,
        )
        .await
        .expect("spawn test session");

    manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return a daemon message");

    let records = manager
        .history
        .list_recent_causal_trace_records("governance_evaluation", 1)
        .await
        .expect("governance causal trace query should succeed");
    assert_eq!(records.len(), 1);

    let selected: serde_json::Value =
        serde_json::from_str(&records[0].selected_json).expect("deserialize selected option");
    assert_eq!(
        selected["option_type"].as_str(),
        Some("governance_evaluation")
    );
    assert!(
        selected["reasoning"].as_str().is_some_and(
            |text| text.contains("require_approval") && text.contains("sandbox_required")
        )
    );

    let factors: Vec<crate::agent::learning::traces::CausalFactor> =
        serde_json::from_str(&records[0].causal_factors_json).expect("deserialize factors");
    assert!(factors
        .iter()
        .any(|factor| factor.description.contains("policy fingerprint")));
    assert!(factors
        .iter()
        .any(|factor| factor.description.contains("triggered constraints")));
    assert!(factors.iter().any(|factor| factor
        .description
        .contains("provenance completeness: complete")));

    let outcome: crate::agent::learning::traces::CausalTraceOutcome =
        serde_json::from_str(&records[0].outcome_json).expect("deserialize outcome");
    assert!(matches!(
        outcome,
        crate::agent::learning::traces::CausalTraceOutcome::Unresolved
    ));

    let filters = vec!["governance_evaluation".to_string()];
    let audits = manager
        .history
        .list_action_audit(Some(filters.as_slice()), None, 5)
        .await
        .expect("governance audit query should succeed");
    assert_eq!(audits.len(), 1);

    let raw_json: serde_json::Value = audits[0]
        .raw_data_json
        .as_deref()
        .map(|text| serde_json::from_str(text).expect("deserialize audit raw json"))
        .expect("raw_data_json should exist");
    assert_eq!(raw_json["verdict_class"].as_str(), Some("require_approval"));
    assert_eq!(raw_json["risk_class"].as_str(), Some("high"));
    assert_eq!(
        raw_json["provenance_completeness"].as_str(),
        Some("complete")
    );
    assert!(raw_json["policy_fingerprint"]
        .as_str()
        .is_some_and(|fingerprint| fingerprint.len() > 8));
    assert!(raw_json["constraints"].as_array().is_some_and(|items| items
        .iter()
        .any(|value| value.as_str() == Some("sandbox_required"))));
}

#[cfg(unix)]
#[tokio::test]
async fn resolve_approval_updates_persisted_resolution() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(
            Some("/bin/sh".to_string()),
            None,
            Some("workspace-a".to_string()),
            None,
            80,
            24,
        )
        .await
        .expect("spawn test session");

    let message = manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return a daemon message");

    let approval_id = match message {
        DaemonMessage::ApprovalRequired { approval, .. } => approval.approval_id,
        other => panic!("expected approval required, got {other:?}"),
    };

    let responses = manager
        .resolve_approval(
            session_id,
            &approval_id,
            amux_protocol::ApprovalDecision::Deny,
        )
        .await
        .expect("approval resolution should succeed");

    assert!(matches!(
        responses.first(),
        Some(DaemonMessage::ApprovalResolved { .. })
    ));
    assert!(matches!(
        responses.get(1),
        Some(DaemonMessage::ManagedCommandRejected { .. })
    ));

    let approval = manager
        .history
        .get_approval_record(&approval_id)
        .await
        .expect("approval lookup should succeed")
        .expect("approval record should exist");
    assert_eq!(approval.resolution.as_deref(), Some("denied"));
    assert!(approval.resolved_at.is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn approve_session_reuses_matching_governance_grant() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(
            Some("/bin/sh".to_string()),
            None,
            Some("workspace-a".to_string()),
            None,
            80,
            24,
        )
        .await
        .expect("spawn test session");

    let request = ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: amux_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: amux_protocol::ManagedCommandSource::Agent,
    };

    let approval_id = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return a daemon message")
    {
        DaemonMessage::ApprovalRequired { approval, .. } => approval.approval_id,
        other => panic!("expected approval required, got {other:?}"),
    };

    let responses = manager
        .resolve_approval(
            session_id,
            &approval_id,
            amux_protocol::ApprovalDecision::ApproveSession,
        )
        .await
        .expect("approval resolution should succeed");
    assert!(matches!(
        responses.first(),
        Some(DaemonMessage::ApprovalResolved { .. })
    ));
    assert!(matches!(
        responses.get(1),
        Some(DaemonMessage::ManagedCommandQueued { .. })
    ));

    let reused = manager
        .execute_managed_command(session_id, request)
        .await
        .expect("matching session grant should allow queueing");
    assert!(matches!(reused, DaemonMessage::ManagedCommandQueued { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn stale_approval_is_invalidated_before_resolution() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(
            Some("/bin/sh".to_string()),
            None,
            Some("workspace-a".to_string()),
            None,
            80,
            24,
        )
        .await
        .expect("spawn test session");

    let approval_id = match manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return a daemon message")
    {
        DaemonMessage::ApprovalRequired { approval, .. } => approval.approval_id,
        other => panic!("expected approval required, got {other:?}"),
    };

    {
        let mut pending = manager.pending_approvals.write().await;
        let entry = pending
            .get_mut(&approval_id)
            .expect("pending approval should exist");
        entry.request.rationale = "different rationale after approval issue".to_string();
    }

    let error = manager
        .resolve_approval(
            session_id,
            &approval_id,
            amux_protocol::ApprovalDecision::ApproveOnce,
        )
        .await
        .expect_err("stale approvals must not be reused");
    assert!(error.to_string().contains("approval is stale"));

    let approval = manager
        .history
        .get_approval_record(&approval_id)
        .await
        .expect("approval lookup should succeed")
        .expect("approval record should exist");
    assert!(approval.invalidated_at.is_some());
    assert!(approval
        .invalidation_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("governance conditions changed")));
}
