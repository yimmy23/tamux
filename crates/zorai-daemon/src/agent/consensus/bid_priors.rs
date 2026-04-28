use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::types::ConsensusBidPrior;
use crate::agent::engine::AgentEngine;

fn clamp_prior(score: f64) -> f64 {
    score.clamp(0.05, 0.95)
}

pub(crate) fn effective_bid_confidence(base_confidence: f64, prior_score: f64) -> f64 {
    (base_confidence + ((prior_score - 0.5) * 0.2)).clamp(0.0, 1.0)
}

fn update_prior(
    existing: Option<ConsensusBidPrior>,
    role: &str,
    outcome: &str,
    now_ms: u64,
) -> ConsensusBidPrior {
    let mut prior = existing.unwrap_or(ConsensusBidPrior {
        role: role.to_string(),
        success_count: 0,
        failure_count: 0,
        prior_score: 0.5,
        last_updated_ms: now_ms,
    });

    prior.role = role.to_string();
    prior.last_updated_ms = now_ms;

    match outcome {
        "success" | "completed" | "accepted" => prior.success_count += 1,
        _ => prior.failure_count += 1,
    }

    let attempts = (prior.success_count + prior.failure_count) as f64;
    let successes = prior.success_count as f64;
    prior.prior_score = clamp_prior((successes + 1.0) / (attempts + 2.0));
    prior
}

impl AgentEngine {
    pub(crate) async fn load_consensus_bid_priors(
        &self,
        roles: &[String],
    ) -> Result<Vec<ConsensusBidPrior>> {
        let roles = roles.to_vec();

        self.history
            .conn
            .call(move |conn| {
                let mut priors = Vec::new();
                let mut stmt = conn.prepare(
                    "SELECT role, success_count, failure_count, prior_score, last_updated_ms
                     FROM consensus_bid_priors
                     WHERE role = ?1",
                )?;
                for role in roles {
                    if let Some(prior) = stmt
                        .query_row(params![role], |row| {
                            Ok(ConsensusBidPrior {
                                role: row.get(0)?,
                                success_count: row.get::<_, i64>(1)? as u64,
                                failure_count: row.get::<_, i64>(2)? as u64,
                                prior_score: row.get(3)?,
                                last_updated_ms: row.get::<_, i64>(4)? as u64,
                            })
                        })
                        .optional()?
                    {
                        priors.push(prior);
                    }
                }
                Ok(priors)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn record_consensus_bid_outcome(
        &self,
        role: &str,
        outcome: &str,
    ) -> Result<()> {
        let role = role.to_string();
        let outcome = outcome.to_string();
        let now_ms = crate::history::now_ts() * 1000;

        self.history
            .conn
            .call(move |conn| {
                let existing = conn
                    .query_row(
                        "SELECT role, success_count, failure_count, prior_score, last_updated_ms
                         FROM consensus_bid_priors
                         WHERE role = ?1",
                        params![&role],
                        |row| {
                            Ok(ConsensusBidPrior {
                                role: row.get(0)?,
                                success_count: row.get::<_, i64>(1)? as u64,
                                failure_count: row.get::<_, i64>(2)? as u64,
                                prior_score: row.get(3)?,
                                last_updated_ms: row.get::<_, i64>(4)? as u64,
                            })
                        },
                    )
                    .optional()?;
                let updated = update_prior(existing, &role, &outcome, now_ms);
                conn.execute(
                    "INSERT INTO consensus_bid_priors (
                        role, success_count, failure_count, prior_score, last_updated_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(role) DO UPDATE SET
                        success_count = excluded.success_count,
                        failure_count = excluded.failure_count,
                        prior_score = excluded.prior_score,
                        last_updated_ms = excluded.last_updated_ms",
                    params![
                        updated.role,
                        updated.success_count as i64,
                        updated.failure_count as i64,
                        updated.prior_score,
                        updated.last_updated_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
