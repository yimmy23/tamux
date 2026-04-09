use amux_protocol::ManagedCommandRequest;

use super::{ConstraintKind, GovernanceConstraint};

pub(crate) fn can_honor_constraints(
    constraints: &[GovernanceConstraint],
    request: &ManagedCommandRequest,
) -> bool {
    let mut constrained_request = request.clone();
    apply_constraints_to_request(&mut constrained_request, constraints);

    constraints.iter().all(|constraint| match constraint.kind {
        ConstraintKind::NetworkDenied => !constrained_request.allow_network,
        ConstraintKind::SandboxRequired => constrained_request.sandbox_enabled,
        ConstraintKind::SerialOnlyExecution
        | ConstraintKind::NetworkRestricted
        | ConstraintKind::FilesystemScopeNarrowed
        | ConstraintKind::TargetScopeCapped
        | ConstraintKind::RetriesDisabled
        | ConstraintKind::RetriesRequireFreshCheckpoint
        | ConstraintKind::ArtifactRetentionElevated
        | ConstraintKind::ManualResumeRequiredAfterCompletion => true,
    })
}

pub(crate) fn apply_constraints_to_request(
    request: &mut ManagedCommandRequest,
    constraints: &[GovernanceConstraint],
) {
    for constraint in constraints {
        match constraint.kind {
            ConstraintKind::NetworkDenied => request.allow_network = false,
            ConstraintKind::SandboxRequired => request.sandbox_enabled = true,
            ConstraintKind::NetworkRestricted
            | ConstraintKind::FilesystemScopeNarrowed
            | ConstraintKind::TargetScopeCapped
            | ConstraintKind::SerialOnlyExecution
            | ConstraintKind::RetriesDisabled
            | ConstraintKind::RetriesRequireFreshCheckpoint
            | ConstraintKind::ArtifactRetentionElevated
            | ConstraintKind::ManualResumeRequiredAfterCompletion => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_protocol::{ManagedCommandSource, SecurityLevel};

    fn request(allow_network: bool, sandbox_enabled: bool) -> ManagedCommandRequest {
        ManagedCommandRequest {
            command: "echo hello".to_string(),
            rationale: "test".to_string(),
            allow_network,
            sandbox_enabled,
            security_level: SecurityLevel::Moderate,
            cwd: Some("/tmp".to_string()),
            language_hint: Some("bash".to_string()),
            source: ManagedCommandSource::Agent,
        }
    }

    #[test]
    fn sandbox_required_is_honored_after_constraint_application() {
        let constraints = vec![GovernanceConstraint::sandbox_required(
            "external side effects must run inside a sandbox",
        )];

        assert!(can_honor_constraints(&constraints, &request(true, false)));
    }

    #[test]
    fn network_denied_is_honored_after_constraint_application() {
        let constraints = vec![GovernanceConstraint::network_denied(
            "network access must be disabled",
        )];

        assert!(can_honor_constraints(&constraints, &request(true, false)));
    }
}
