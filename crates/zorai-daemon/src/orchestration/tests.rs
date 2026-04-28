use super::*;
use crate::governance::GovernanceConstraint;
use zorai_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

fn request() -> ManagedCommandRequest {
    ManagedCommandRequest {
        command: "echo hello".to_string(),
        rationale: "verify plan".to_string(),
        allow_network: false,
        sandbox_enabled: true,
        security_level: SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: ManagedCommandSource::Agent,
    }
}

#[test]
fn plan_managed_command_run_uses_workspace_target_and_reused_lane() {
    let session_id = "11111111-1111-1111-1111-111111111111";
    let plan = plan_managed_command_run(
        "exec_1",
        Some(session_id),
        Some("workspace-a"),
        &request(),
        &[GovernanceConstraint::sandbox_required(
            "sandbox must be enabled",
        )],
    );

    assert_eq!(plan.run_id, "exec_1");
    assert_eq!(plan.targets.len(), 1);
    assert_eq!(plan.targets[0].target_id, "workspace-a");
    assert_eq!(plan.targets[0].target_kind, TargetKind::Workspace);
    assert_eq!(plan.targets[0].resolved_binding, session_id);
    assert_eq!(plan.targets[0].binding_origin, BindingOrigin::Reused);
    assert_eq!(plan.candidate_lanes[0].lane_id, session_id);
    assert_eq!(
        plan.candidate_lanes[0].binding_origin,
        BindingOrigin::Reused
    );
    assert!(plan.targets[0]
        .constraints_inherited_from_target_or_environment
        .iter()
        .any(|constraint| constraint.contains("sandbox_required")));
    assert_eq!(plan.primary_stage().sync_mode, SyncMode::Barrier);
    assert_eq!(
        plan.primary_stage().participating_lanes,
        vec![session_id.to_string()]
    );
    assert!(plan
        .primary_stage()
        .entry_criteria
        .iter()
        .any(|criterion| criterion.contains("governance checkpoint admissible")));
}

#[test]
fn plan_managed_command_run_falls_back_to_terminal_target_without_workspace() {
    let session_id = "22222222-2222-2222-2222-222222222222";
    let plan = plan_managed_command_run("exec_2", Some(session_id), None, &request(), &[]);

    assert_eq!(plan.targets[0].target_kind, TargetKind::TerminalSession);
    assert_eq!(plan.targets[0].target_id, session_id);
    assert_eq!(plan.lane_ids(), vec![session_id.to_string()]);
    assert_eq!(plan.sync_policies, vec![SyncMode::Barrier]);
}

#[test]
fn plan_managed_command_run_allocates_placeholder_lane_when_binding_is_not_resolved() {
    let plan = plan_managed_command_run("exec_3", None, Some("workspace-a"), &request(), &[]);

    assert_eq!(plan.targets[0].target_kind, TargetKind::Workspace);
    assert_eq!(
        plan.targets[0].binding_origin,
        BindingOrigin::NewlyAllocated
    );
    assert_eq!(plan.targets[0].resolved_binding, "planned_lane:exec_3");
    assert_eq!(
        plan.candidate_lanes[0].binding_origin,
        BindingOrigin::NewlyAllocated
    );
    assert_eq!(
        plan.primary_stage().participating_lanes,
        vec!["planned_lane:exec_3".to_string()]
    );
}

#[test]
fn plan_managed_command_run_maps_initiator_from_request_source() {
    let mut request = request();
    request.source = ManagedCommandSource::Human;

    let plan = plan_managed_command_run(
        "exec_4",
        Some("33333333-3333-3333-3333-333333333333"),
        Some("workspace-a"),
        &request,
        &[],
    );
    assert_eq!(plan.initiator, "operator");
}

#[test]
fn plan_managed_command_run_labels_inherited_constraints_with_rationale() {
    let plan = plan_managed_command_run(
        "exec_5",
        Some("44444444-4444-4444-4444-444444444444"),
        Some("workspace-a"),
        &request(),
        &[
            GovernanceConstraint::network_denied("network must remain disabled"),
            GovernanceConstraint::serial_only(
                "high concurrency blast radius requires serialization",
            ),
        ],
    );

    assert!(plan.targets[0]
        .constraints_inherited_from_target_or_environment
        .contains(&"network_denied: network must remain disabled".to_string()));
    assert!(plan.targets[0]
        .constraints_inherited_from_target_or_environment
        .contains(
            &"serial_only_execution: high concurrency blast radius requires serialization"
                .to_string()
        ));
}
