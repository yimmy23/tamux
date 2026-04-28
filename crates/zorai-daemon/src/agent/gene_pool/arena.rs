use std::collections::BTreeMap;

use crate::history::SkillVariantRecord;

use super::types::GenePoolCrossBreedProposal;

pub(crate) fn build_cross_breed_proposals(
    variants: &[SkillVariantRecord],
    arena_scores: &BTreeMap<String, f64>,
    now_ms: u64,
) -> Vec<GenePoolCrossBreedProposal> {
    let mut by_skill = BTreeMap::<String, Vec<&SkillVariantRecord>>::new();
    for variant in variants
        .iter()
        .filter(|variant| variant.status != "archived")
    {
        by_skill
            .entry(variant.skill_name.clone())
            .or_default()
            .push(variant);
    }

    let mut proposals = Vec::new();
    for (skill_name, records) in by_skill {
        if records.len() < 2 {
            continue;
        }
        let strong = records
            .into_iter()
            .filter(|record| {
                arena_scores
                    .get(&record.variant_id)
                    .copied()
                    .unwrap_or_default()
                    >= 0.72
            })
            .collect::<Vec<_>>();
        if strong.len() < 2 {
            continue;
        }
        let left = strong[0];
        let right = strong[1];
        let tag_union = left
            .context_tags
            .iter()
            .chain(right.context_tags.iter())
            .collect::<std::collections::BTreeSet<_>>();
        let co_usage_rate = (tag_union.len().min(6) as f64) / 6.0;
        proposals.push(GenePoolCrossBreedProposal {
            left_parent_variant_id: left.variant_id.clone(),
            right_parent_variant_id: right.variant_id.clone(),
            skill_name,
            co_usage_rate,
            proposed_at_ms: now_ms,
        });
    }
    proposals
}
