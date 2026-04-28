use super::types::PersistedRoleAssignment;
use crate::agent::collaboration::ConsensusBid;

pub(crate) fn build_role_assignment(
    parent_task_id: &str,
    round_id: u64,
    ranked_bids: &[ConsensusBid],
    assigned_at_ms: u64,
) -> PersistedRoleAssignment {
    let primary = ranked_bids.first().expect("ranked bids must not be empty");
    let reviewer = ranked_bids.get(1);
    let observers = ranked_bids
        .iter()
        .skip(2)
        .filter(|bid| bid.confidence >= 0.4)
        .map(|bid| bid.task_id.clone())
        .collect::<Vec<_>>();

    PersistedRoleAssignment {
        task_id: parent_task_id.to_string(),
        round_id,
        primary_agent_id: primary.task_id.clone(),
        reviewer_agent_id: reviewer.map(|bid| bid.task_id.clone()),
        observers,
        assigned_at_ms,
        outcome: None,
    }
}
