use rand::distributions::{Distribution, WeightedIndex};
use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::agent::handoff::audit::CapabilityScoreRow;
use crate::agent::handoff::profiles::compute_learned_routing_weights;
use crate::agent::handoff::SpecialistProfile;
use crate::agent::morphogenesis::types::MorphogenesisAffinity;
use crate::agent::types::RoutingConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RankedRoutingCandidate {
    pub profile_idx: usize,
    pub profile_id: String,
    pub learned_weight: f64,
    pub morphogenesis_affinity: f64,
    pub final_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RoutingSnapshot {
    pub required_tags: Vec<String>,
    pub ranked_candidates: Vec<RankedRoutingCandidate>,
}

fn affinity_for_profile(
    profile: &SpecialistProfile,
    required_tags: &[String],
    morphogenesis: &[MorphogenesisAffinity],
) -> f64 {
    let matches = morphogenesis
        .iter()
        .filter(|affinity| {
            affinity.agent_id == profile.id
                && required_tags.iter().any(|tag| tag == &affinity.domain)
        })
        .map(|affinity| affinity.affinity_score)
        .collect::<Vec<_>>();

    if matches.is_empty() {
        0.0
    } else {
        matches.iter().sum::<f64>() / matches.len() as f64
    }
}

pub(crate) fn build_routing_snapshot(
    profiles: &[SpecialistProfile],
    required_tags: &[String],
    score_rows: &[CapabilityScoreRow],
    morphogenesis: &[MorphogenesisAffinity],
    routing: &RoutingConfig,
    now_ms: u64,
) -> RoutingSnapshot {
    let learned =
        compute_learned_routing_weights(profiles, required_tags, score_rows, routing, now_ms)
            .into_iter()
            .map(|entry| (entry.profile_idx, entry.weight))
            .collect::<std::collections::HashMap<_, _>>();

    let mut ranked_candidates = profiles
        .iter()
        .enumerate()
        .filter(|(_, profile)| {
            required_tags.iter().any(|required_tag| {
                profile
                    .capabilities
                    .iter()
                    .any(|capability| capability.tag == *required_tag)
            })
        })
        .map(|(profile_idx, profile)| {
            let learned_weight = learned.get(&profile_idx).copied().unwrap_or_default();
            let morphogenesis_affinity =
                affinity_for_profile(profile, required_tags, morphogenesis);
            let final_weight = learned_weight * (1.0 + morphogenesis_affinity);
            RankedRoutingCandidate {
                profile_idx,
                profile_id: profile.id.clone(),
                learned_weight,
                morphogenesis_affinity,
                final_weight,
            }
        })
        .collect::<Vec<_>>();

    ranked_candidates.sort_by(|left, right| {
        right
            .final_weight
            .partial_cmp(&left.final_weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    RoutingSnapshot {
        required_tags: required_tags.to_vec(),
        ranked_candidates,
    }
}

pub(crate) fn select_snapshot_candidate(
    snapshot: &RoutingSnapshot,
    confidence_threshold: f64,
) -> Option<(usize, f64)> {
    if !confidence_threshold.is_finite() {
        return None;
    }

    let eligible = snapshot
        .ranked_candidates
        .iter()
        .filter(|candidate| candidate.final_weight >= confidence_threshold)
        .collect::<Vec<_>>();

    match eligible.len() {
        0 => None,
        1 => Some((eligible[0].profile_idx, eligible[0].final_weight)),
        _ => {
            let weights = eligible
                .iter()
                .map(|candidate| candidate.final_weight)
                .collect::<Vec<_>>();
            let distribution = WeightedIndex::new(&weights).ok()?;
            let selected = eligible[distribution.sample(&mut thread_rng())];
            Some((selected.profile_idx, selected.final_weight))
        }
    }
}
