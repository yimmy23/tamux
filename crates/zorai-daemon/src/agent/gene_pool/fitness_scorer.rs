use crate::history::SkillVariantRecord;

use super::types::GenePoolFitnessSnapshot;

pub(crate) fn build_fitness_history(
    variants: &[SkillVariantRecord],
    recorded_at_ms: u64,
) -> Vec<GenePoolFitnessSnapshot> {
    variants
        .iter()
        .map(|record| GenePoolFitnessSnapshot {
            variant_id: record.variant_id.clone(),
            recorded_at_ms,
            fitness_score: record.fitness_score,
            use_count: record.use_count,
            success_rate: record.success_rate(),
        })
        .collect()
}
