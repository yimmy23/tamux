#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TargetKind {
    Workspace,
    TerminalSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BindingOrigin {
    Reused,
    NewlyAllocated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SyncMode {
    Barrier,
    Quorum,
    Race,
    Pipeline,
    Serial,
    BestEffortFanOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TargetResolutionRecord {
    pub target_id: String,
    pub target_kind: TargetKind,
    pub requested_scope: String,
    pub resolved_binding: String,
    pub binding_origin: BindingOrigin,
    pub constraints_inherited_from_target_or_environment: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CandidateLane {
    pub lane_id: String,
    pub target_id: String,
    pub binding_origin: BindingOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StageContract {
    pub stage_id: String,
    pub participating_lanes: Vec<String>,
    pub input_artifacts: Vec<String>,
    pub sync_mode: SyncMode,
    pub entry_criteria: Vec<String>,
    pub success_criteria: Vec<String>,
    pub per_lane_timeout_policy: String,
    pub cancellation_policy: String,
    pub expected_output_contract: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ManagedCommandRunPlan {
    pub run_id: String,
    pub objective: String,
    pub initiator: String,
    pub targets: Vec<TargetResolutionRecord>,
    pub candidate_lanes: Vec<CandidateLane>,
    pub stage_graph: Vec<StageContract>,
    pub sync_policies: Vec<SyncMode>,
    pub provenance_root: String,
}

impl ManagedCommandRunPlan {
    pub fn primary_stage(&self) -> &StageContract {
        self.stage_graph
            .first()
            .expect("managed command plan always contains one stage")
    }

    pub fn target_ids(&self) -> Vec<String> {
        self.targets
            .iter()
            .map(|target| target.target_id.clone())
            .collect()
    }

    pub fn lane_ids(&self) -> Vec<String> {
        self.primary_stage().participating_lanes.clone()
    }
}
