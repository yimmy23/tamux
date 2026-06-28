use anyhow::Result;

use super::affinity_tracker::{apply_decay, apply_outcome};
use crate::history::db;

fn row_to_affinity(row: &db::Row) -> Result<MorphogenesisAffinity> {
    Ok(MorphogenesisAffinity {
        agent_id: row.get(0)?,
        domain: row.get(1)?,
        affinity_score: row.get(2)?,
        task_count: row.get::<i64>(3)? as u64,
        success_count: row.get::<i64>(4)? as u64,
        failure_count: row.get::<i64>(5)? as u64,
        last_updated_ms: row.get::<i64>(6)? as u64,
    })
}
use super::soul_adaptor::{apply_specialization_section, build_soul_adaptation};
use super::types::{
    AdaptationType, AffinityUpdate, MorphogenesisAffinity, MorphogenesisOutcome, SoulAdaptation,
};
use crate::agent::engine::AgentEngine;
use crate::agent::memory::ensure_memory_files_for_scope;
use crate::agent::task_prompt::memory_paths_for_scope;

const MORPHOGENESIS_DECAY_FLOOR: f64 = 0.01;

impl AgentEngine {
    async fn load_all_morphogenesis_affinities_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<MorphogenesisAffinity>> {
        let agent_id = agent_id.to_string();
        let now_ms = crate::history::now_ts() * 1000;

        let rows = self
            .history
            .read_db
            .query(
                "SELECT agent_id, domain, affinity_score, task_count, success_count, failure_count, last_updated_ms
                 FROM morphogenesis_affinities
                 WHERE agent_id = ?1",
                db::db_params![agent_id],
            )
            .await?;
        let mut affinities = Vec::new();
        for row in &rows {
            affinities.push(apply_decay(
                row_to_affinity(row)?,
                now_ms,
                MORPHOGENESIS_DECAY_FLOOR,
            ));
        }
        Ok(affinities)
    }

    pub(crate) async fn load_morphogenesis_affinities(
        &self,
        domains: &[String],
    ) -> Result<Vec<MorphogenesisAffinity>> {
        let domains = domains.to_vec();
        let now_ms = crate::history::now_ts() * 1000;

        let mut affinities = Vec::new();
        for domain in domains {
            let rows = self
                .history
                .read_db
                .query(
                    "SELECT agent_id, domain, affinity_score, task_count, success_count, failure_count, last_updated_ms
                     FROM morphogenesis_affinities
                     WHERE domain = ?1",
                    db::db_params![domain],
                )
                .await?;
            for row in &rows {
                affinities.push(apply_decay(
                    row_to_affinity(row)?,
                    now_ms,
                    MORPHOGENESIS_DECAY_FLOOR,
                ));
            }
        }
        Ok(affinities)
    }

    pub(crate) async fn load_morphogenesis_affinity_updates(
        &self,
        agent_id: &str,
        domain: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AffinityUpdate>> {
        let agent_id = agent_id.to_string();
        let domain = domain.map(str::to_string);
        let limit = limit.max(1) as i64;
        let rows = if let Some(domain) = domain {
            self.history
                .read_db
                .query(
                    "SELECT agent_id, domain, old_affinity, new_affinity, trigger_type, task_id, updated_at_ms
                     FROM affinity_updates_log
                     WHERE agent_id = ?1 AND domain = ?2
                     ORDER BY updated_at_ms DESC, id DESC
                     LIMIT ?3",
                    db::db_params![agent_id, domain, limit],
                )
                .await?
        } else {
            self.history
                .read_db
                .query(
                    "SELECT agent_id, domain, old_affinity, new_affinity, trigger_type, task_id, updated_at_ms
                     FROM affinity_updates_log
                     WHERE agent_id = ?1
                     ORDER BY updated_at_ms DESC, id DESC
                     LIMIT ?2",
                    db::db_params![agent_id, limit],
                )
                .await?
        };
        rows.iter()
            .map(|row| {
                Ok(AffinityUpdate {
                    agent_id: row.get(0)?,
                    domain: row.get(1)?,
                    old_affinity: row.get(2)?,
                    new_affinity: row.get(3)?,
                    trigger_type: row.get(4)?,
                    task_id: row.get(5)?,
                    updated_at_ms: row.get::<i64>(6)?.max(0) as u64,
                })
            })
            .collect()
    }

    pub(crate) async fn load_soul_adaptations(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<SoulAdaptation>> {
        let agent_id = agent_id.to_string();
        let limit = limit.max(1) as i64;
        let rows = self
            .history
            .read_db
            .query(
                "SELECT agent_id, domain, adaptation_type, soul_snippet, old_soul_hash, new_soul_hash, created_at_ms
                 FROM soul_adaptations_log
                 WHERE agent_id = ?1
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![agent_id, limit],
            )
            .await?;
        rows.iter()
            .map(|row| {
                let adaptation_type = match row.get::<String>(2)?.as_str() {
                    "added" => AdaptationType::Added,
                    "removed" => AdaptationType::Removed,
                    _ => AdaptationType::Updated,
                };
                Ok(SoulAdaptation {
                    agent_id: row.get(0)?,
                    domain: row.get(1)?,
                    adaptation_type,
                    soul_snippet: row.get(3)?,
                    old_soul_hash: row.get(4)?,
                    new_soul_hash: row.get(5)?,
                    created_at_ms: row.get::<i64>(6)?.max(0) as u64,
                })
            })
            .collect()
    }

    pub(crate) async fn record_morphogenesis_outcome(
        &self,
        agent_id: &str,
        domains: &[String],
        outcome: MorphogenesisOutcome,
    ) -> Result<()> {
        let agent_id = agent_id.to_string();
        let domains = domains.to_vec();
        let now_ms = (crate::history::now_ts() as i64) * 1000;
        let trigger_type = match outcome {
            MorphogenesisOutcome::Success => "success",
            MorphogenesisOutcome::Partial => "revision",
            MorphogenesisOutcome::Failure => "failure",
        }
        .to_string();

        let agent_id_for_db = agent_id.clone();
        let mut updates = Vec::new();
        for domain in domains {
            let existing = self
                .history
                .conn_db
                .query_opt(
                    "SELECT affinity_score, task_count, success_count, failure_count, last_updated_ms
                     FROM morphogenesis_affinities
                     WHERE agent_id = ?1 AND domain = ?2",
                    db::db_params![agent_id_for_db.clone(), domain.clone()],
                )
                .await?
                .map(|row| {
                    Ok::<_, anyhow::Error>(MorphogenesisAffinity {
                        agent_id: agent_id_for_db.clone(),
                        domain: domain.clone(),
                        affinity_score: row.get(0)?,
                        task_count: row.get::<i64>(1)? as u64,
                        success_count: row.get::<i64>(2)? as u64,
                        failure_count: row.get::<i64>(3)? as u64,
                        last_updated_ms: row.get::<i64>(4)? as u64,
                    })
                })
                .transpose()?;

            let old_affinity = existing
                .as_ref()
                .map(|value| value.affinity_score)
                .unwrap_or(0.0);
            let updated =
                apply_outcome(existing, &agent_id_for_db, &domain, outcome, now_ms as u64);
            self.history
                .conn_db
                .execute(
                    "INSERT INTO morphogenesis_affinities (
                        agent_id, domain, affinity_score, task_count, success_count, failure_count, last_updated_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(agent_id, domain) DO UPDATE SET
                        affinity_score = excluded.affinity_score,
                        task_count = excluded.task_count,
                        success_count = excluded.success_count,
                        failure_count = excluded.failure_count,
                        last_updated_ms = excluded.last_updated_ms",
                    db::db_params![
                        updated.agent_id.clone(),
                        updated.domain.clone(),
                        updated.affinity_score,
                        updated.task_count as i64,
                        updated.success_count as i64,
                        updated.failure_count as i64,
                        updated.last_updated_ms as i64,
                    ],
                )
                .await?;
            self.history
                .conn_db
                .execute(
                    "INSERT INTO affinity_updates_log (
                        agent_id, domain, old_affinity, new_affinity, trigger_type, task_id, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
                    db::db_params![
                        updated.agent_id.clone(),
                        updated.domain.clone(),
                        old_affinity,
                        updated.affinity_score,
                        trigger_type.clone(),
                        updated.last_updated_ms as i64,
                    ],
                )
                .await?;
            updates.push((domain, old_affinity, updated));
        }

        for (domain, _old_affinity, updated) in updates {
            let all_affinities = self
                .load_all_morphogenesis_affinities_for_agent(&agent_id)
                .await?;
            let old_affinity = all_affinities
                .iter()
                .find(|affinity| affinity.domain == domain)
                .cloned()
                .map(|mut affinity| {
                    affinity.affinity_score = _old_affinity;
                    affinity
                });
            self.apply_morphogenesis_soul_adaptation(
                &agent_id,
                &domain,
                old_affinity.as_ref(),
                &updated,
                &all_affinities,
            )
            .await?;
        }

        Ok(())
    }

    async fn apply_morphogenesis_soul_adaptation(
        &self,
        agent_id: &str,
        domain: &str,
        old_affinity: Option<&MorphogenesisAffinity>,
        updated_affinity: &MorphogenesisAffinity,
        all_affinities: &[MorphogenesisAffinity],
    ) -> Result<()> {
        ensure_memory_files_for_scope(&self.data_dir, agent_id).await?;
        let paths = memory_paths_for_scope(&self.data_dir, agent_id);
        let current_soul = tokio::fs::read_to_string(&paths.soul_path)
            .await
            .unwrap_or_default();
        let Some(adaptation) = build_soul_adaptation(
            agent_id,
            domain,
            old_affinity,
            updated_affinity,
            &current_soul,
            all_affinities,
            crate::history::now_ts() * 1000,
        ) else {
            return Ok(());
        };

        let (updated_soul, _) = apply_specialization_section(&current_soul, all_affinities);
        tokio::fs::write(&paths.soul_path, updated_soul).await?;

        let adaptation_type = match adaptation.adaptation_type {
            AdaptationType::Added => "added",
            AdaptationType::Removed => "removed",
            AdaptationType::Updated => "updated",
        }
        .to_string();
        let agent_id = adaptation.agent_id.clone();
        let domain = adaptation.domain.clone();
        let snippet = adaptation.soul_snippet.clone();
        let old_hash = adaptation.old_soul_hash.clone();
        let new_hash = adaptation.new_soul_hash.clone();
        let created_at_ms = adaptation.created_at_ms as i64;

        self.history
            .conn_db
            .execute(
                "INSERT INTO soul_adaptations_log (
                    agent_id, domain, adaptation_type, soul_snippet, old_soul_hash, new_soul_hash, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![agent_id, domain, adaptation_type, snippet, old_hash, new_hash, created_at_ms],
            )
            .await?;
        Ok(())
    }
}
