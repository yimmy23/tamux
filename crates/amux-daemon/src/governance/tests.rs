use amux_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

use super::{
    compute_policy_fingerprint, effective_constraints, evaluate_governance,
    governance_input_for_managed_command, GovernanceInput, ProvenanceCompleteness,
    ProvenanceStatus, RiskDimensions, TransitionKind, VerdictClass,
};

fn make_request(
    command: &str,
    allow_network: bool,
    sandbox_enabled: bool,
) -> ManagedCommandRequest {
    ManagedCommandRequest {
        command: command.to_string(),
        rationale: "test rationale".to_string(),
        allow_network,
        sandbox_enabled,
        security_level: SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: ManagedCommandSource::Agent,
    }
}

#[test]
fn fingerprint_is_stable_across_target_ordering() {
    let request = make_request("echo hello", false, true);
    let mut left = governance_input_for_managed_command(
        "exec_left",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    left.lane_ids = vec!["lane-b".to_string(), "lane-a".to_string()];
    left.target_ids = vec!["target-b".to_string(), "target-a".to_string()];

    let mut right = left.clone();
    right.lane_ids.reverse();
    right.target_ids.reverse();

    assert_eq!(
        compute_policy_fingerprint(&left),
        compute_policy_fingerprint(&right)
    );
}

#[test]
fn insufficient_provenance_defers_side_effecting_transition() {
    let request = make_request("rm -rf /tmp/demo", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_risky",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.provenance_status = ProvenanceStatus {
        completeness: ProvenanceCompleteness::Insufficient,
        missing_evidence: vec!["missing command provenance".to_string()],
    };

    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::Defer);
}

#[test]
fn high_risk_requires_approval() {
    let request = make_request("sudo terraform destroy", true, false);
    let input = governance_input_for_managed_command(
        "exec_high",
        &request,
        Some("workspace-a".to_string()),
        None,
    );

    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert!(constraints
        .iter()
        .any(|constraint| matches!(constraint.kind, super::ConstraintKind::SandboxRequired)));
}

#[test]
fn high_concurrency_adds_serial_constraint() {
    let input = GovernanceInput {
        run_id: None,
        task_id: None,
        thread_id: None,
        goal_run_id: None,
        transition_kind: TransitionKind::ManagedCommandDispatch,
        stage_id: None,
        lane_ids: vec!["lane-a".to_string(), "lane-b".to_string()],
        target_ids: vec!["workspace-a".to_string()],
        requested_action_summary: "parallel fanout dispatch".to_string(),
        intent_summary: "exercise high concurrency rule".to_string(),
        risk_dimensions: RiskDimensions {
            destructiveness: 2,
            scope: 3,
            reversibility: 2,
            privilege: 1,
            externality: 1,
            concurrency: 8,
        },
        blast_radius: super::BlastRadiusEstimate {
            lane_scope: "two lanes".to_string(),
            stage_scope: "stage-a".to_string(),
            run_scope: "workspace".to_string(),
        },
        environment_facts: super::EnvironmentFacts {
            sandbox_available: true,
            sandbox_enabled: true,
            network_allowed: false,
            filesystem_scope: Some("/tmp".to_string()),
            workspace_id: Some("workspace-a".to_string()),
            host_type: None,
            privilege_posture: None,
        },
        approval_context: super::ApprovalContext::default(),
        retry_or_rebind_history: Vec::new(),
        provenance_status: ProvenanceStatus::complete(),
        rollback_or_compensation_hints: super::CompensationHints::unknown(),
        initiator: super::GovernanceInitiator::Agent,
    };

    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert!(constraints
        .iter()
        .any(|constraint| matches!(constraint.kind, super::ConstraintKind::SerialOnlyExecution)));
}
