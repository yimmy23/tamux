use anyhow::Result;
use rusqlite::params;

use super::types::PersistedConsensusBid;
use crate::agent::collaboration::{BidAvailability, CollaborationSession};
use crate::agent::engine::AgentEngine;

pub(crate) fn consensus_round_id(session: &CollaborationSession) -> u64 {
    session
        .call_metadata
        .as_ref()
        .map(|metadata| metadata.called_at)
        .unwrap_or(session.updated_at.max(1))
}

fn availability_label(availability: &BidAvailability) -> &'static str {
    match availability {
        BidAvailability::Available => "available",
        BidAvailability::Busy => "busy",
        BidAvailability::Unavailable => "unavailable",
    }
}

pub(crate) fn build_persisted_bid(
    session: &CollaborationSession,
    agent_id: &str,
    confidence: f64,
    availability: &BidAvailability,
    submitted_at_ms: u64,
) -> PersistedConsensusBid {
    let role = session
        .agents
        .iter()
        .find(|agent| agent.task_id == agent_id)
        .map(|agent| agent.role.clone())
        .unwrap_or_else(|| "specialist".to_string());
    PersistedConsensusBid {
        task_id: session.parent_task_id.clone(),
        round_id: consensus_round_id(session),
        agent_id: agent_id.to_string(),
        confidence,
        reasoning: format!("role {role} bid at confidence {:.2}", confidence),
        availability: availability_label(availability).to_string(),
        domain_affinity: session
            .agents
            .iter()
            .find(|agent| agent.task_id == agent_id)
            .map(|agent| agent.confidence)
            .unwrap_or(confidence),
        submitted_at_ms,
    }
}

impl AgentEngine {
    pub(crate) async fn persist_consensus_bid(&self, bid: PersistedConsensusBid) -> Result<()> {
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO consensus_bids (
                        task_id, round_id, agent_id, confidence, reasoning, availability, domain_affinity, submitted_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        bid.task_id,
                        bid.round_id as i64,
                        bid.agent_id,
                        bid.confidence,
                        bid.reasoning,
                        bid.availability,
                        bid.domain_affinity,
                        bid.submitted_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
