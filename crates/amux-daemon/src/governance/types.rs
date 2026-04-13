#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    RunAdmission,
    LaneAdmission,
    StageAdvance,
    LaneRetry,
    ResumeFromBlocked,
    CompensationEntry,
    FinalDisposition,
    ManagedCommandDispatch,
    ApprovalReuseCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictClass {
    Allow,
    AllowWithConstraints,
    RequireApproval,
    Defer,
    Deny,
    HaltAndIsolate,
    AllowOnlyWithCompensationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    SandboxRequired,
    NetworkDenied,
    NetworkRestricted,
    FilesystemScopeNarrowed,
    TargetScopeCapped,
    SerialOnlyExecution,
    RetriesDisabled,
    RetriesRequireFreshCheckpoint,
    ArtifactRetentionElevated,
    ManualResumeRequiredAfterCompletion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceConstraint {
    pub kind: ConstraintKind,
    pub value: Option<String>,
    pub rationale: Option<String>,
}

impl GovernanceConstraint {
    pub fn serial_only(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::SerialOnlyExecution,
            value: None,
            rationale: Some(rationale.into()),
        }
    }

    pub fn sandbox_required(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::SandboxRequired,
            value: None,
            rationale: Some(rationale.into()),
        }
    }

    pub fn network_denied(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::NetworkDenied,
            value: None,
            rationale: Some(rationale.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskDimensions {
    pub destructiveness: u8,
    pub scope: u8,
    pub reversibility: u8,
    pub privilege: u8,
    pub externality: u8,
    pub concurrency: u8,
}

impl RiskDimensions {
    pub fn max_score(&self) -> u8 {
        [
            self.destructiveness,
            self.scope,
            self.reversibility,
            self.privilege,
            self.externality,
            self.concurrency,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    pub fn from_managed_command(request: &amux_protocol::ManagedCommandRequest) -> Self {
        let command = request.command.to_ascii_lowercase();
        let destructive = if command.contains("rm -rf")
            || command.contains("git reset --hard")
            || command.contains("terraform destroy")
            || command.contains("kubectl delete")
        {
            9
        } else if command.contains("rm ") || command.contains("mv ") || command.contains("cp ") {
            4
        } else {
            1
        };

        let scope = if request.allow_network { 6 } else { 2 };
        let reversibility = if destructive >= 8 { 9 } else { 3 };
        let privilege = if command.contains("sudo") || command.contains("systemctl") {
            8
        } else {
            2
        };
        let externality = if request.allow_network { 7 } else { 1 };
        let concurrency = if request.command.contains("&&") || request.command.contains(";") {
            5
        } else {
            1
        };

        Self {
            destructiveness: destructive,
            scope,
            reversibility,
            privilege,
            externality,
            concurrency,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlastRadiusEstimate {
    pub lane_scope: String,
    pub stage_scope: String,
    pub run_scope: String,
}

impl BlastRadiusEstimate {
    pub fn for_managed_command(
        request: &amux_protocol::ManagedCommandRequest,
        workspace_id: Option<String>,
    ) -> Self {
        let run_scope = if request.allow_network {
            "network and workspace".to_string()
        } else if workspace_id.is_some() {
            "workspace".to_string()
        } else {
            "current session".to_string()
        };

        Self {
            lane_scope: "current terminal lane".to_string(),
            stage_scope: "managed command dispatch".to_string(),
            run_scope,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentFacts {
    pub sandbox_available: bool,
    pub sandbox_enabled: bool,
    pub network_allowed: bool,
    pub filesystem_scope: Option<String>,
    pub workspace_id: Option<String>,
    pub host_type: Option<String>,
    pub privilege_posture: Option<String>,
}

impl EnvironmentFacts {
    pub fn for_managed_command(
        request: &amux_protocol::ManagedCommandRequest,
        workspace_id: Option<String>,
    ) -> Self {
        Self {
            sandbox_available: true,
            sandbox_enabled: request.sandbox_enabled,
            network_allowed: request.allow_network,
            filesystem_scope: request.cwd.clone(),
            workspace_id,
            host_type: None,
            privilege_posture: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalContext {
    pub prior_approval_ids: Vec<String>,
    pub approval_fresh: bool,
    pub conditions_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceCompleteness {
    Complete,
    Partial,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceStatus {
    pub completeness: ProvenanceCompleteness,
    pub missing_evidence: Vec<String>,
}

impl ProvenanceStatus {
    pub fn complete() -> Self {
        Self {
            completeness: ProvenanceCompleteness::Complete,
            missing_evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompensationHints {
    pub rollback_feasible: bool,
    pub compensation_feasible: bool,
    pub hints: Vec<String>,
}

impl CompensationHints {
    pub fn unknown() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceInitiator {
    Operator,
    Agent,
    GoalRunner,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceInput {
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub transition_kind: TransitionKind,
    pub stage_id: Option<String>,
    pub lane_ids: Vec<String>,
    pub target_ids: Vec<String>,
    pub requested_action_summary: String,
    pub intent_summary: String,
    pub risk_dimensions: RiskDimensions,
    pub blast_radius: BlastRadiusEstimate,
    pub environment_facts: EnvironmentFacts,
    pub approval_context: ApprovalContext,
    pub retry_or_rebind_history: Vec<String>,
    pub provenance_status: ProvenanceStatus,
    pub rollback_or_compensation_hints: CompensationHints,
    pub initiator: GovernanceInitiator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRequirement {
    pub scope_summary: String,
    pub expires_at: Option<u64>,
    pub policy_fingerprint: String,
    pub constraints: Vec<GovernanceConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentScope {
    Lane,
    Stage,
    Run,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompensationRequirement {
    pub required: bool,
    pub reason: String,
    pub plan_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceVerdict {
    pub verdict_class: VerdictClass,
    pub risk_class: RiskClass,
    pub rationale: Vec<String>,
    pub constraints: Vec<GovernanceConstraint>,
    pub approval_requirement: Option<ApprovalRequirement>,
    pub containment_scope: Option<ContainmentScope>,
    pub compensation_requirement: Option<CompensationRequirement>,
    pub freshness_window_secs: Option<u64>,
    pub policy_fingerprint: String,
}

impl GovernanceVerdict {
    pub fn allow(policy_fingerprint: String, risk_class: RiskClass) -> Self {
        Self {
            verdict_class: VerdictClass::Allow,
            risk_class,
            rationale: Vec::new(),
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    pub fn allow_with_optional_constraints(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        constraints: Vec<GovernanceConstraint>,
    ) -> Self {
        let verdict_class = if constraints.is_empty() {
            VerdictClass::Allow
        } else {
            VerdictClass::AllowWithConstraints
        };

        Self {
            verdict_class,
            risk_class,
            rationale,
            constraints,
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    pub fn require_approval(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        constraints: Vec<GovernanceConstraint>,
    ) -> Self {
        Self {
            verdict_class: VerdictClass::RequireApproval,
            risk_class,
            rationale: rationale.clone(),
            constraints: constraints.clone(),
            approval_requirement: Some(ApprovalRequirement {
                scope_summary: "managed transition".to_string(),
                expires_at: None,
                policy_fingerprint: policy_fingerprint.clone(),
                constraints,
            }),
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: Some(900),
            policy_fingerprint,
        }
    }

    pub fn defer(policy_fingerprint: String, rationale: Vec<String>) -> Self {
        Self {
            verdict_class: VerdictClass::Defer,
            risk_class: RiskClass::High,
            rationale,
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }
}

pub(crate) fn effective_constraints(verdict: &GovernanceVerdict) -> Vec<GovernanceConstraint> {
    let mut constraints = verdict.constraints.clone();

    if let Some(requirement) = &verdict.approval_requirement {
        for constraint in &requirement.constraints {
            if !constraints.contains(constraint) {
                constraints.push(constraint.clone());
            }
        }
    }

    constraints
}
