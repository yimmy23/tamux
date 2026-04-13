#![allow(dead_code)]

use amux_protocol::{ManagedCommandRequest, ManagedCommandSource};

use super::{
    BindingOrigin, CandidateLane, ManagedCommandRunPlan, StageContract, SyncMode, TargetKind,
    TargetResolutionRecord,
};
use crate::governance::{ConstraintKind, GovernanceConstraint};

pub(crate) fn plan_managed_command_run(
    execution_id: &str,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
    request: &ManagedCommandRequest,
    inherited_constraints: &[GovernanceConstraint],
) -> ManagedCommandRunPlan {
    let (lane_id, binding_origin) = match session_id {
        Some(session_id) => (session_id.to_string(), BindingOrigin::Reused),
        None => (
            format!("planned_lane:{execution_id}"),
            BindingOrigin::NewlyAllocated,
        ),
    };
    let inherited_constraints = inherited_constraints.iter().map(constraint_label).collect();
    let target = match workspace_id {
        Some(workspace_id) => TargetResolutionRecord {
            target_id: workspace_id.to_string(),
            target_kind: TargetKind::Workspace,
            requested_scope: workspace_id.to_string(),
            resolved_binding: lane_id.clone(),
            binding_origin: binding_origin.clone(),
            constraints_inherited_from_target_or_environment: inherited_constraints,
        },
        None => TargetResolutionRecord {
            target_id: lane_id.clone(),
            target_kind: TargetKind::TerminalSession,
            requested_scope: lane_id.clone(),
            resolved_binding: lane_id.clone(),
            binding_origin: binding_origin.clone(),
            constraints_inherited_from_target_or_environment: inherited_constraints,
        },
    };

    let objective = if request.rationale.trim().is_empty() {
        format!("dispatch managed command: {}", request.command)
    } else {
        request.rationale.trim().to_string()
    };

    let initiator = initiator_label(request.source).to_string();
    let stage = StageContract {
        stage_id: "managed_dispatch".to_string(),
        participating_lanes: vec![lane_id.clone()],
        input_artifacts: Vec::new(),
        sync_mode: SyncMode::Barrier,
        entry_criteria: vec![
            "target binding complete".to_string(),
            "governance checkpoint admissible".to_string(),
        ],
        success_criteria: vec![
            "command dispatched to admitted lane".to_string(),
            "dispatch provenance persisted".to_string(),
        ],
        per_lane_timeout_policy: "inherit managed command runtime defaults".to_string(),
        cancellation_policy: "lane scoped cooperative cancellation".to_string(),
        expected_output_contract: vec![
            "stdout_stream".to_string(),
            "stderr_stream".to_string(),
            "exit_status".to_string(),
            "timing_summary".to_string(),
        ],
    };

    ManagedCommandRunPlan {
        run_id: execution_id.to_string(),
        objective,
        initiator,
        targets: vec![target.clone()],
        candidate_lanes: vec![CandidateLane {
            lane_id: lane_id.clone(),
            target_id: target.target_id.clone(),
            binding_origin,
        }],
        stage_graph: vec![stage],
        sync_policies: vec![SyncMode::Barrier],
        provenance_root: execution_id.to_string(),
    }
}

fn initiator_label(source: ManagedCommandSource) -> &'static str {
    match source {
        ManagedCommandSource::Human => "operator",
        ManagedCommandSource::Agent => "agent",
        ManagedCommandSource::Replay => "system",
        ManagedCommandSource::Gateway => "gateway",
    }
}

fn constraint_label(constraint: &GovernanceConstraint) -> String {
    format!(
        "{}{}",
        match constraint.kind {
            ConstraintKind::SandboxRequired => "sandbox_required",
            ConstraintKind::NetworkDenied => "network_denied",
            ConstraintKind::NetworkRestricted => "network_restricted",
            ConstraintKind::FilesystemScopeNarrowed => "filesystem_scope_narrowed",
            ConstraintKind::TargetScopeCapped => "target_scope_capped",
            ConstraintKind::SerialOnlyExecution => "serial_only_execution",
            ConstraintKind::RetriesDisabled => "retries_disabled",
            ConstraintKind::RetriesRequireFreshCheckpoint => "retries_require_fresh_checkpoint",
            ConstraintKind::ArtifactRetentionElevated => "artifact_retention_elevated",
            ConstraintKind::ManualResumeRequiredAfterCompletion => {
                "manual_resume_required_after_completion"
            }
        },
        constraint
            .rationale
            .as_ref()
            .map(|rationale| format!(": {rationale}"))
            .unwrap_or_default()
    )
}
