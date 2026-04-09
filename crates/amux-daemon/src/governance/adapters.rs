use amux_protocol::ManagedCommandRequest;

use super::{
    ApprovalContext, BlastRadiusEstimate, CompensationHints, EnvironmentFacts, GovernanceInitiator,
    GovernanceInput, ProvenanceStatus, RiskDimensions, TransitionKind,
};

pub(crate) fn governance_input_for_managed_command(
    _execution_id: &str,
    request: &ManagedCommandRequest,
    workspace_id: Option<String>,
    session_id: Option<String>,
) -> GovernanceInput {
    GovernanceInput {
        run_id: None,
        task_id: None,
        thread_id: None,
        goal_run_id: None,
        transition_kind: TransitionKind::ManagedCommandDispatch,
        stage_id: None,
        lane_ids: session_id.into_iter().collect(),
        target_ids: workspace_id.clone().into_iter().collect(),
        requested_action_summary: request.command.clone(),
        intent_summary: request.rationale.clone(),
        risk_dimensions: RiskDimensions::from_managed_command(request),
        blast_radius: BlastRadiusEstimate::for_managed_command(request, workspace_id.clone()),
        environment_facts: EnvironmentFacts::for_managed_command(request, workspace_id),
        approval_context: ApprovalContext::default(),
        retry_or_rebind_history: Vec::new(),
        provenance_status: ProvenanceStatus::complete(),
        rollback_or_compensation_hints: CompensationHints::unknown(),
        initiator: GovernanceInitiator::Agent,
    }
}
