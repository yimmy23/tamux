#![allow(dead_code)]

use crate::governance::{
    BlastRadiusEstimate, CompensationHints, EnvironmentFacts, GovernanceInitiator, GovernanceInput,
    GovernanceVerdict, RiskDimensions, TransitionKind,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HarnessRecordKind {
    Observation,
    Belief,
    Goal,
    WorldState,
    Tension,
    Commitment,
    Effect,
    Verification,
    Procedure,
    EffectContract,
}

impl HarnessRecordKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Belief => "belief",
            Self::Goal => "goal",
            Self::WorldState => "world_state",
            Self::Tension => "tension",
            Self::Commitment => "commitment",
            Self::Effect => "effect",
            Self::Verification => "verification",
            Self::Procedure => "procedure",
            Self::EffectContract => "effect_contract",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ObservationKind {
    OperatorRequest,
    SystemSignal,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BeliefKind {
    Working,
    Constraint,
    GoalContext,
    WorldModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TensionKind {
    Contradiction,
    InformationGap,
    RiskEscalation,
    StaleCommitment,
    Drift,
    Opportunity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommitmentStatus {
    Proposed,
    Active,
    Blocked,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EffectStatus {
    Planned,
    Dispatched,
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationStatus {
    Pending,
    Verified,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProcedureStatus {
    Candidate,
    Learned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EffectExecutionKind {
    ReadOnly,
    Mutating,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationGateKind {
    GovernanceVerdict,
    EffectPersisted,
    DesiredStateMatch,
    EffectOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommitmentRole {
    InterpreterCartographer,
    DeliberatorStrategist,
    Executor,
    VerifierAuditor,
    SkepticCritic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ObservationRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: ObservationKind,
    pub summary: String,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct BeliefRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: BeliefKind,
    pub summary: String,
    pub confidence: f64,
    pub source_observation_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GoalRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub summary: String,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct WorldStateRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub summary: String,
    pub focus: String,
    pub observed_state: serde_json::Value,
    pub desired_state: Option<serde_json::Value>,
    pub contradictions: Vec<String>,
    pub unknowns: Vec<String>,
    pub risk_flags: Vec<String>,
    pub opportunities: Vec<String>,
    pub stale_commitment_ids: Vec<String>,
    pub active_tension_ids: Vec<String>,
    pub next_step_hint: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct TensionRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: TensionKind,
    pub summary: String,
    pub priority: u8,
    pub source_belief_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct VerificationGateRecord {
    pub name: String,
    pub kind: VerificationGateKind,
    pub description: String,
    pub required: bool,
    pub evidence_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct EffectContractRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub summary: String,
    pub execution_kind: EffectExecutionKind,
    pub reversible: bool,
    pub risk_hint: String,
    pub blast_radius_hint: String,
    pub preconditions: Vec<String>,
    pub expected_effects: Vec<String>,
    pub verification_strategy: String,
    pub verification_gates: Vec<VerificationGateRecord>,
    pub governance_input: GovernanceInput,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RoleAssessment {
    pub role: CommitmentRole,
    pub stance: String,
    pub summary: String,
    pub concerns: Vec<String>,
    pub recommended_action: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CommitmentRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub summary: String,
    pub rationale: String,
    pub status: CommitmentStatus,
    pub source_tension_id: Option<String>,
    pub source_world_state_id: Option<String>,
    pub effect_contract_id: Option<String>,
    pub expected_effects: Vec<String>,
    pub verification_plan: String,
    pub critique_summary: Option<String>,
    pub role_assessments: Vec<RoleAssessment>,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct EffectRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub commitment_id: String,
    pub effect_contract_id: Option<String>,
    pub summary: String,
    pub status: EffectStatus,
    pub dispatch_success: bool,
    pub output: serde_json::Value,
    pub governance_verdict: GovernanceVerdict,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct VerificationGateResult {
    pub gate_name: String,
    pub passed: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct VerificationRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub effect_id: String,
    pub summary: String,
    pub status: VerificationStatus,
    pub verified: bool,
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ProcedureRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub summary: String,
    pub status: ProcedureStatus,
    #[serde(default)]
    pub trace_signature: String,
    pub applicability: Vec<String>,
    pub outcome_summary: String,
    #[serde(default)]
    pub verified_outcome: bool,
    #[serde(default)]
    pub successful_trace_count: u32,
    #[serde(default)]
    pub failed_trace_count: u32,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub preferred_effect_order: Vec<String>,
    #[serde(default)]
    pub source_verification_id: Option<String>,
    #[serde(default)]
    pub details: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct HarnessRecordEnvelope {
    pub entry_id: String,
    pub entity_id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: HarnessRecordKind,
    pub status: Option<String>,
    pub summary: String,
    pub payload: serde_json::Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub(crate) struct HarnessStateProjection {
    pub observations: Vec<ObservationRecord>,
    pub beliefs: Vec<BeliefRecord>,
    pub goals: Vec<GoalRecord>,
    pub world_states: Vec<WorldStateRecord>,
    pub tensions: Vec<TensionRecord>,
    pub commitments: Vec<CommitmentRecord>,
    pub effects: Vec<EffectRecord>,
    pub verifications: Vec<VerificationRecord>,
    pub procedures: Vec<ProcedureRecord>,
    pub effect_contracts: Vec<EffectContractRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct HarnessLoopInput {
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub observation_summary: String,
    pub observation_details: serde_json::Value,
    pub goal_summary: String,
    pub desired_state: Option<serde_json::Value>,
    pub preferred_effect_kind: Option<EffectExecutionKind>,
    pub allow_network: bool,
    pub sandbox_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct HarnessLoopResult {
    pub projection: HarnessStateProjection,
    pub world_state_id: String,
    pub selected_tension_ids: Vec<String>,
    pub selected_commitment_id: String,
    pub effect_contract_id: String,
    pub effect_id: String,
    pub verification_id: String,
    pub procedure_id: String,
}

pub(crate) fn governance_input_for_harness_effect(
    thread_id: Option<&str>,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    summary: &str,
    execution_kind: EffectExecutionKind,
    allow_network: bool,
    sandbox_enabled: bool,
    risk_flags: &[String],
) -> GovernanceInput {
    let mut risk_dimensions = match execution_kind {
        EffectExecutionKind::ReadOnly => RiskDimensions {
            destructiveness: 1,
            scope: if allow_network { 4 } else { 1 },
            reversibility: 1,
            privilege: 1,
            externality: if allow_network { 5 } else { 1 },
            concurrency: 1,
        },
        EffectExecutionKind::Mutating => RiskDimensions {
            destructiveness: 4,
            scope: if allow_network { 6 } else { 4 },
            reversibility: 4,
            privilege: 2,
            externality: if allow_network { 7 } else { 2 },
            concurrency: 1,
        },
    };

    if risk_flags.iter().any(|flag| flag.contains("privilege")) {
        risk_dimensions.privilege = risk_dimensions.privilege.max(8);
    }
    if risk_flags
        .iter()
        .any(|flag| flag.contains("approval") || flag.contains("high_blast_radius"))
    {
        risk_dimensions.destructiveness = risk_dimensions.destructiveness.max(8);
        risk_dimensions.scope = risk_dimensions.scope.max(7);
        risk_dimensions.reversibility = risk_dimensions.reversibility.max(7);
    }
    if risk_flags
        .iter()
        .any(|flag| flag.contains("external") || flag.contains("network"))
    {
        risk_dimensions.externality = risk_dimensions.externality.max(7);
        risk_dimensions.scope = risk_dimensions.scope.max(6);
    }

    GovernanceInput {
        run_id: task_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        thread_id: thread_id.map(str::to_string),
        goal_run_id: goal_run_id.map(str::to_string),
        transition_kind: TransitionKind::StageAdvance,
        stage_id: Some(
            match execution_kind {
                EffectExecutionKind::ReadOnly => "harness_read_only_effect",
                EffectExecutionKind::Mutating => "harness_mutating_effect",
            }
            .to_string(),
        ),
        lane_ids: Vec::new(),
        target_ids: thread_id.into_iter().map(str::to_string).collect(),
        requested_action_summary: summary.to_string(),
        intent_summary: match execution_kind {
            EffectExecutionKind::ReadOnly => {
                "governed read-only harness verification and inspection".to_string()
            }
            EffectExecutionKind::Mutating => {
                "governed mutating harness correction attempt".to_string()
            }
        },
        risk_dimensions,
        blast_radius: BlastRadiusEstimate {
            lane_scope: "harness loop".to_string(),
            stage_scope: match execution_kind {
                EffectExecutionKind::ReadOnly => "diagnostic effect",
                EffectExecutionKind::Mutating => "corrective effect",
            }
            .to_string(),
            run_scope: if allow_network {
                "goal-local plus external surface".to_string()
            } else {
                "goal-local".to_string()
            },
        },
        environment_facts: EnvironmentFacts {
            sandbox_available: true,
            sandbox_enabled,
            network_allowed: allow_network,
            filesystem_scope: None,
            workspace_id: None,
            host_type: Some("daemon".to_string()),
            privilege_posture: Some("unprivileged".to_string()),
        },
        approval_context: Default::default(),
        retry_or_rebind_history: Vec::new(),
        provenance_status: crate::governance::ProvenanceStatus::complete(),
        rollback_or_compensation_hints: CompensationHints {
            rollback_feasible: execution_kind == EffectExecutionKind::ReadOnly,
            compensation_feasible: true,
            hints: match execution_kind {
                EffectExecutionKind::ReadOnly => {
                    vec!["read-only audit can be re-run without compensating action".to_string()]
                }
                EffectExecutionKind::Mutating => vec![
                    "bounded correction should emit enough state to support compensating replay"
                        .to_string(),
                ],
            },
        },
        initiator: GovernanceInitiator::GoalRunner,
    }
}

pub(crate) fn placeholder_governance_input(
    thread_id: Option<&str>,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    summary: &str,
) -> GovernanceInput {
    governance_input_for_harness_effect(
        thread_id,
        goal_run_id,
        task_id,
        summary,
        EffectExecutionKind::ReadOnly,
        false,
        true,
        &[],
    )
}
