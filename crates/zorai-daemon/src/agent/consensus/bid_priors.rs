use anyhow::Result;

use super::types::ConsensusBidPrior;
use crate::agent::engine::AgentEngine;
use crate::history::db;

fn row_to_bid_prior(row: &db::Row) -> Result<ConsensusBidPrior> {
    Ok(ConsensusBidPrior {
        role: row.get(0)?,
        success_count: row.get::<i64>(1)? as u64,
        failure_count: row.get::<i64>(2)? as u64,
        prior_score: row.get(3)?,
        last_updated_ms: row.get::<i64>(4)? as u64,
    })
}

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

        let mut priors = Vec::new();
        for role in roles {
            if let Some(row) = self
                .history
                .read_db
                .query_opt(
                    "SELECT role, success_count, failure_count, prior_score, last_updated_ms
                     FROM consensus_bid_priors
                     WHERE role = ?1",
                    db::db_params![role],
                )
                .await?
            {
                priors.push(row_to_bid_prior(&row)?);
            }
        }
        Ok(priors)
    }

    pub(crate) async fn record_consensus_bid_outcome(
        &self,
        role: &str,
        outcome: &str,
    ) -> Result<()> {
        let role = role.to_string();
        let outcome = outcome.to_string();
        let now_ms = crate::history::now_ts() * 1000;

        let existing = self
            .history
            .conn_db
            .query_opt(
                "SELECT role, success_count, failure_count, prior_score, last_updated_ms
                 FROM consensus_bid_priors
                 WHERE role = ?1",
                db::db_params![role.clone()],
            )
            .await?
            .map(|row| row_to_bid_prior(&row))
            .transpose()?;
        let updated = update_prior(existing, &role, &outcome, now_ms);
        self.history
            .conn_db
            .execute(
                "INSERT INTO consensus_bid_priors (
                    role, success_count, failure_count, prior_score, last_updated_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(role) DO UPDATE SET
                    success_count = excluded.success_count,
                    failure_count = excluded.failure_count,
                    prior_score = excluded.prior_score,
                    last_updated_ms = excluded.last_updated_ms",
                db::db_params![
                    updated.role,
                    updated.success_count as i64,
                    updated.failure_count as i64,
                    updated.prior_score,
                    updated.last_updated_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }
}
