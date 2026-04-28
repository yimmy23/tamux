use sha2::{Digest, Sha256};

use super::GovernanceInput;

pub(crate) fn compute_policy_fingerprint(input: &GovernanceInput) -> String {
    let mut lane_ids = input.lane_ids.clone();
    lane_ids.sort();

    let mut target_ids = input.target_ids.clone();
    target_ids.sort();

    let canonical = serde_json::json!({
        "transition_kind": &input.transition_kind,
        "requested_action_summary": &input.requested_action_summary,
        "intent_summary": &input.intent_summary,
        "lane_ids": lane_ids,
        "target_ids": target_ids,
        "risk_dimensions": &input.risk_dimensions,
        "blast_radius": &input.blast_radius,
        "environment_facts": &input.environment_facts,
        "provenance_status": &input.provenance_status,
        "rollback_or_compensation_hints": &input.rollback_or_compensation_hints,
        "initiator": &input.initiator,
    });

    let encoded = serde_json::to_vec(&canonical).expect("governance fingerprint json");
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}
