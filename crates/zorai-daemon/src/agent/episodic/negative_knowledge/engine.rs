#![allow(dead_code)]

use super::*;
use crate::agent::now_millis;

impl AgentEngine {
    pub(crate) async fn add_negative_constraint(
        &self,
        constraint: NegativeConstraint,
    ) -> Result<()> {
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let now_ms = now_millis();
        let agent_id_for_db = agent_id.clone();
        let incoming_constraint = constraint.clone();

        let (persisted_constraint, propagated_constraints) = self
            .history
            .conn
            .call(move |conn| {
                let include_legacy = crate::agent::is_main_agent_scope(&agent_id_for_db) as i64;
                let mut active_constraints = load_all_active_constraints(
                    conn,
                    &agent_id_for_db,
                    include_legacy,
                    now_ms as i64,
                )?;

                let merge_idx = active_constraints.iter().position(|existing| {
                    constraints_match_for_merge(existing, &incoming_constraint)
                });

                let (persisted_constraint, reached_dead) = if let Some(idx) = merge_idx {
                    let existing = active_constraints[idx].clone();
                    let merged = merge_constraint_evidence(&existing, &incoming_constraint);
                    let reached_dead = existing.state != ConstraintState::Dead
                        && merged.state == ConstraintState::Dead;
                    active_constraints[idx] = merged.clone();
                    (merged, reached_dead)
                } else {
                    let reached_dead = incoming_constraint.state == ConstraintState::Dead;
                    active_constraints.push(incoming_constraint.clone());
                    (incoming_constraint.clone(), reached_dead)
                };

                let propagated_constraints = if reached_dead {
                    let propagated =
                        propagate_dead_constraint(&persisted_constraint, &active_constraints);
                    for propagated_constraint in &propagated {
                        if let Some(idx) = active_constraints
                            .iter()
                            .position(|existing| existing.id == propagated_constraint.id)
                        {
                            active_constraints[idx] = propagated_constraint.clone();
                        }
                    }
                    propagated
                } else {
                    Vec::new()
                };

                let tx = conn.unchecked_transaction()?;
                persist_constraint_in_transaction(&tx, &persisted_constraint, &agent_id_for_db)?;
                for propagated_constraint in &propagated_constraints {
                    persist_constraint_in_transaction(
                        &tx,
                        propagated_constraint,
                        &agent_id_for_db,
                    )?;
                }
                tx.commit()?;

                Ok((persisted_constraint, propagated_constraints))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store
            .cached_constraints
            .retain(|constraint| is_constraint_active(constraint, now_ms));

        let mut touched_constraints = propagated_constraints;
        touched_constraints.push(persisted_constraint.clone());

        for touched_constraint in touched_constraints {
            if let Some(existing) = store
                .cached_constraints
                .iter_mut()
                .find(|existing| existing.id == touched_constraint.id)
            {
                *existing = touched_constraint;
            } else {
                store.cached_constraints.push(touched_constraint);
            }
        }

        tracing::info!(subject = %persisted_constraint.subject, "Added negative knowledge constraint");

        Ok(())
    }

    pub(crate) async fn record_negative_knowledge_from_episode(
        &self,
        episode: &Episode,
    ) -> Result<()> {
        if episode.outcome != EpisodeOutcome::Failure || episode.root_cause.is_none() {
            return Ok(());
        }

        let config = self.config.read().await;
        let constraint_ttl_days = config.episodic.constraint_ttl_days;
        drop(config);

        let now_ms = now_millis();
        let valid_until = now_ms + constraint_ttl_days * 86400 * 1000;
        let constraint = build_direct_constraint_from_episode(
            episode,
            now_ms,
            valid_until,
            format!("nc_{}", uuid::Uuid::new_v4()),
        );

        self.add_negative_constraint(constraint).await
    }

    pub(crate) async fn record_negative_knowledge_from_tool_failure(
        &self,
        scope_hint: Option<&str>,
        tool_name: &str,
        args_summary: &str,
        failure_description: &str,
    ) -> Result<()> {
        let config = self.config.read().await;
        let constraint_ttl_days = config.episodic.constraint_ttl_days;
        drop(config);

        let scope_hint = scope_hint.map(str::trim).filter(|value| !value.is_empty());
        let args_summary = args_summary.trim();
        let tool_signature = if args_summary.is_empty() {
            tool_name.to_string()
        } else {
            format!("{tool_name}({args_summary})")
        };
        let subject = scope_hint
            .map(|scope| format!("{scope}: {tool_signature}"))
            .unwrap_or(tool_signature);
        let description = failure_description
            .trim()
            .chars()
            .take(400)
            .collect::<String>();

        let now_ms = now_millis();
        let valid_until = now_ms + constraint_ttl_days * 86400 * 1000;
        let constraint = NegativeConstraint {
            id: format!("nc_{}", uuid::Uuid::new_v4()),
            episode_id: None,
            constraint_type: ConstraintType::RuledOut,
            subject: subject.clone(),
            solution_class: scope_hint.map(str::to_string),
            description: if description.is_empty() {
                format!("{tool_name} failed")
            } else {
                description
            },
            confidence: 0.7,
            state: ConstraintState::Dying,
            evidence_count: 1,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: normalize_subject_tokens(&subject),
            valid_until: Some(valid_until),
            created_at: now_ms,
        };

        self.add_negative_constraint(constraint).await
    }

    pub(crate) async fn query_active_constraints(
        &self,
        entity_filter: Option<&str>,
    ) -> Result<Vec<NegativeConstraint>> {
        let now_ms = crate::agent::now_millis() as i64;
        let filter = entity_filter.map(|s| format!("%{s}%"));
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;

        self.history
            .conn
            .call(move |conn| {
                if let Some(ref pattern) = filter {
                    let mut stmt = conn.prepare(
                        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                                description, confidence, state, evidence_count, direct_observation,
                                derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
                         FROM negative_knowledge
                         WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
                         AND (valid_until IS NULL OR valid_until > ?3)
                         AND (subject LIKE ?4 OR solution_class LIKE ?4)
                         AND deleted_at IS NULL
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows = stmt.query_map(
                        params![agent_id, include_legacy, now_ms, pattern],
                        row_to_constraint,
                    )?;
                    let mut constraints = Vec::new();
                    for row in rows {
                        constraints.push(row?);
                    }
                    Ok(constraints)
                } else {
                    let mut stmt = conn.prepare(
                        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                                description, confidence, state, evidence_count, direct_observation,
                                derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
                         FROM negative_knowledge
                         WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
                           AND (valid_until IS NULL OR valid_until > ?3)
                           AND deleted_at IS NULL
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows = stmt.query_map(
                        params![agent_id, include_legacy, now_ms],
                        row_to_constraint,
                    )?;
                    let mut constraints = Vec::new();
                    for row in rows {
                        constraints.push(row?);
                    }
                    Ok(constraints)
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn expire_negative_constraints(&self) -> Result<usize> {
        let now_ms = crate::agent::now_millis() as i64;

        let deleted = self
            .history
            .conn
            .call(move |conn| {
                let count = conn.execute(
                    "UPDATE negative_knowledge SET deleted_at = ?2 WHERE valid_until IS NOT NULL AND valid_until <= ?1 AND deleted_at IS NULL",
                    params![now_ms, now_ms],
                )?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if deleted > 0 {
            let now_ms = now_millis();
            let scope_id = crate::agent::agent_identity::current_agent_scope_id();
            let mut stores = self.episodic_store.write().await;
            let store = stores.entry(scope_id).or_default();
            store
                .cached_constraints
                .retain(|c| is_constraint_active(c, now_ms));
        }

        Ok(deleted)
    }

    pub(crate) async fn refresh_constraint_cache(&self) -> Result<()> {
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        let now_ms = crate::agent::now_millis() as i64;
        let constraints = self
            .history
            .conn
            .call(move |conn| {
                Ok(load_all_active_constraints(
                    conn,
                    &agent_id,
                    include_legacy,
                    now_ms,
                )?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store.cached_constraints = constraints;
        Ok(())
    }
}
