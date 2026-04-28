use crate::agent::stalled_turns::{StalledTurnCandidate, ThreadStallObservation};
use crate::agent::types::RoutingConfig;
use crate::agent::{
    background_workers::domain_memory::MemoryWorkerSnapshot,
    background_workers::domain_routing::RoutingSnapshot,
    context::structural_memory::ThreadStructuralMemory,
    gene_pool::types::GenePoolRuntimeSnapshot,
    handoff::{audit::CapabilityScoreRow, SpecialistProfile},
    morphogenesis::types::MorphogenesisAffinity,
    semantic_env::SemanticPackageSummary,
};
use crate::history::{ExecutionTraceRow, SkillVariantRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundWorkerKind {
    Safety,
    Rhythm,
    Anticipatory,
    Learning,
    Routing,
    Memory,
}

impl BackgroundWorkerKind {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn cli_arg(self) -> &'static str {
        match self {
            Self::Safety => "__zorai-background-worker-safety",
            Self::Rhythm => "__zorai-background-worker-rhythm",
            Self::Anticipatory => "__zorai-background-worker-anticipatory",
            Self::Learning => "__zorai-background-worker-learning",
            Self::Routing => "__zorai-background-worker-routing",
            Self::Memory => "__zorai-background-worker-memory",
        }
    }

    pub(crate) fn from_cli_arg(arg: &str) -> Option<Self> {
        match arg {
            "__zorai-background-worker-safety" => Some(Self::Safety),
            "__zorai-background-worker-rhythm" => Some(Self::Rhythm),
            "__zorai-background-worker-anticipatory" => Some(Self::Anticipatory),
            "__zorai-background-worker-learning" => Some(Self::Learning),
            "__zorai-background-worker-routing" => Some(Self::Routing),
            "__zorai-background-worker-memory" => Some(Self::Memory),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SafetyDecision {
    Retry { candidate: StalledTurnCandidate },
    Escalate { candidate: StalledTurnCandidate },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundWorkerCommand {
    Ping,
    TickSafety {
        observations: Vec<ThreadStallObservation>,
        candidates: Vec<StalledTurnCandidate>,
        now_ms: u64,
    },
    TickRouting {
        profiles: Vec<SpecialistProfile>,
        required_tags: Vec<String>,
        score_rows: Vec<CapabilityScoreRow>,
        morphogenesis: Vec<MorphogenesisAffinity>,
        routing: RoutingConfig,
        now_ms: u64,
    },
    TickMemory {
        thread_id: Option<String>,
        task_id: Option<String>,
        structural_memory: Option<ThreadStructuralMemory>,
        semantic_packages: Vec<SemanticPackageSummary>,
        now_ms: u64,
    },
    TickLearning {
        successful_traces: Vec<ExecutionTraceRow>,
        variants: Vec<SkillVariantRecord>,
        now_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundWorkerResult {
    Pong { kind: BackgroundWorkerKind },
    SafetyTick { decisions: Vec<SafetyDecision> },
    RoutingTick { snapshot: RoutingSnapshot },
    MemoryTick { snapshot: MemoryWorkerSnapshot },
    LearningTick { snapshot: GenePoolRuntimeSnapshot },
    Noop { kind: BackgroundWorkerKind },
    Error { message: String },
}
