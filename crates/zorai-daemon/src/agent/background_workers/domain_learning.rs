use crate::agent::gene_pool::runtime::build_gene_pool_runtime_snapshot;
use crate::agent::gene_pool::types::GenePoolRuntimeSnapshot;
use crate::history::{ExecutionTraceRow, SkillVariantRecord};

pub(crate) fn build_learning_snapshot(
    successful_traces: &[ExecutionTraceRow],
    variants: &[SkillVariantRecord],
    now_ms: u64,
) -> GenePoolRuntimeSnapshot {
    build_gene_pool_runtime_snapshot(successful_traces, variants, now_ms)
}
