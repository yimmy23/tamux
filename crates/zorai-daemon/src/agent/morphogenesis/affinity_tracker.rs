use super::types::{MorphogenesisAffinity, MorphogenesisOutcome};

const DEFAULT_STARTING_AFFINITY: f64 = 0.25;
const MIN_AFFINITY: f64 = 0.01;
const MAX_AFFINITY: f64 = 1.0;
const DECAY_HALF_LIFE_MS: u64 = 3 * 24 * 60 * 60 * 1000;

fn clamp_affinity(value: f64, floor: f64) -> f64 {
    value.clamp(floor.max(MIN_AFFINITY), MAX_AFFINITY)
}

pub(crate) fn apply_outcome(
    existing: Option<MorphogenesisAffinity>,
    agent_id: &str,
    domain: &str,
    outcome: MorphogenesisOutcome,
    now_ms: u64,
) -> MorphogenesisAffinity {
    let mut affinity = existing.unwrap_or(MorphogenesisAffinity {
        agent_id: agent_id.to_string(),
        domain: domain.to_string(),
        affinity_score: DEFAULT_STARTING_AFFINITY,
        task_count: 0,
        success_count: 0,
        failure_count: 0,
        last_updated_ms: now_ms,
    });

    affinity.agent_id = agent_id.to_string();
    affinity.domain = domain.to_string();
    affinity.task_count += 1;
    affinity.last_updated_ms = now_ms;

    let delta = match outcome {
        MorphogenesisOutcome::Success => {
            affinity.success_count += 1;
            0.20
        }
        MorphogenesisOutcome::Partial => 0.05,
        MorphogenesisOutcome::Failure => {
            affinity.failure_count += 1;
            -0.18
        }
    };

    affinity.affinity_score = clamp_affinity(affinity.affinity_score + delta, MIN_AFFINITY);
    affinity
}

pub(crate) fn apply_decay(
    affinity: MorphogenesisAffinity,
    now_ms: u64,
    floor: f64,
) -> MorphogenesisAffinity {
    if now_ms <= affinity.last_updated_ms {
        return affinity;
    }

    let elapsed_ms = now_ms - affinity.last_updated_ms;
    let decay_steps = elapsed_ms as f64 / DECAY_HALF_LIFE_MS as f64;
    let retained = 0.5_f64.powf(decay_steps);
    let floor = floor.max(MIN_AFFINITY);
    let decayed_score = floor + ((affinity.affinity_score - floor).max(0.0) * retained);

    MorphogenesisAffinity {
        affinity_score: clamp_affinity(decayed_score, floor),
        last_updated_ms: now_ms,
        ..affinity
    }
}
