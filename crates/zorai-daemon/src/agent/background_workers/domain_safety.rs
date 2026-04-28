use super::protocol::SafetyDecision;
use crate::agent::stalled_turns::{StalledTurnCandidate, ThreadStallObservation};
use std::collections::{HashMap, HashSet};

pub(crate) fn evaluate_tick(
    observations: Vec<ThreadStallObservation>,
    candidates: Vec<StalledTurnCandidate>,
    now_ms: u64,
) -> Vec<SafetyDecision> {
    let observed_ids = observations
        .iter()
        .map(|observation| observation.thread_id.clone())
        .collect::<HashSet<_>>();
    let mut candidates_by_thread = candidates
        .into_iter()
        .filter(|candidate| observed_ids.contains(&candidate.thread_id))
        .map(|candidate| (candidate.thread_id.clone(), candidate))
        .collect::<HashMap<_, _>>();
    let mut decisions = Vec::new();

    for observation in observations {
        let candidate = candidates_by_thread
            .entry(observation.thread_id.clone())
            .or_insert_with(|| StalledTurnCandidate::from_observation(&observation));

        if candidate.last_message_id != observation.last_message_id {
            *candidate = StalledTurnCandidate::from_observation(&observation);
        }

        if now_ms < candidate.next_evaluation_at {
            continue;
        }

        if candidate.escalation_ready(now_ms) {
            decisions.push(SafetyDecision::Escalate {
                candidate: candidate.clone(),
            });
        } else {
            decisions.push(SafetyDecision::Retry {
                candidate: candidate.clone(),
            });
        }
    }

    decisions
}
