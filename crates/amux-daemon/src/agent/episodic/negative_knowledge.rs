//! Negative knowledge constraint graph: tracks ruled-out approaches,
//! impossible combinations, and known limitations with TTL expiry.

use super::{ConstraintState, ConstraintType, Episode, EpisodeOutcome, NegativeConstraint};
use crate::agent::engine::AgentEngine;

use anyhow::Result;
use rusqlite::params;

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Normalize a subject into lowercase alphanumeric tokens.
/// Drops tokens shorter than 3 characters, then sorts and dedupes.
pub(crate) fn normalize_subject_tokens(subject: &str) -> Vec<String> {
    let mut tokens: Vec<String> = subject
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect();

    tokens.sort();
    tokens.dedup();
    tokens
}

/// Build a stable deduped subject key for exact normalized comparisons.
pub(crate) fn normalized_subject_key(subject: &str) -> String {
    normalize_subject_tokens(subject).join(" ")
}

/// Compute the next monotonic constraint state from accumulated evidence.
pub(crate) fn next_constraint_state(
    current: ConstraintState,
    evidence_count: u32,
    direct_observation: bool,
    confidence: f64,
) -> ConstraintState {
    if current == ConstraintState::Dead {
        return ConstraintState::Dead;
    }

    if (direct_observation && confidence >= 0.85) || evidence_count >= 3 {
        return ConstraintState::Dead;
    }

    if evidence_count >= 2 {
        return ConstraintState::Dying;
    }

    current
}

/// Determine whether two constraints are identical enough to merge.
pub(crate) fn constraints_match_for_merge(a: &NegativeConstraint, b: &NegativeConstraint) -> bool {
    let a_key = normalized_subject_key(&a.subject);
    let b_key = normalized_subject_key(&b.subject);

    !a_key.is_empty() && a_key == b_key && a.solution_class == b.solution_class
}

fn shared_normalized_subject_token_count(a: &NegativeConstraint, b: &NegativeConstraint) -> usize {
    let a_tokens = normalize_subject_tokens(&a.subject);
    let b_tokens = normalize_subject_tokens(&b.subject);

    a_tokens
        .iter()
        .filter(|token| b_tokens.binary_search(token).is_ok())
        .count()
}

/// Determine whether two constraints are related enough for propagation.
pub(crate) fn related_for_propagation(
    source: &NegativeConstraint,
    target: &NegativeConstraint,
) -> bool {
    let shared_tokens = shared_normalized_subject_token_count(source, target);

    match (&source.solution_class, &target.solution_class) {
        (Some(source_class), Some(target_class)) => {
            source_class == target_class && shared_tokens >= 2
        }
        (None, None) => shared_tokens >= 3,
        _ => false,
    }
}

pub(crate) fn build_direct_constraint_from_episode(
    episode: &Episode,
    now_ms: u64,
    valid_until: u64,
    id: String,
) -> NegativeConstraint {
    let subject = if episode.summary.len() > 200 {
        format!("{}...", &episode.summary[..197])
    } else {
        episode.summary.clone()
    };
    let confidence = episode.confidence.unwrap_or(0.7);

    NegativeConstraint {
        id,
        episode_id: Some(episode.id.clone()),
        constraint_type: ConstraintType::RuledOut,
        related_subject_tokens: normalize_subject_tokens(&subject),
        subject,
        solution_class: episode.solution_class.clone(),
        description: episode.root_cause.clone().unwrap_or_default(),
        confidence,
        state: next_constraint_state(ConstraintState::Dying, 1, true, confidence),
        evidence_count: 1,
        direct_observation: true,
        derived_from_constraint_ids: Vec::new(),
        valid_until: Some(valid_until),
        created_at: now_ms,
    }
}

fn effective_related_subject_tokens(constraint: &NegativeConstraint) -> Vec<String> {
    if constraint.related_subject_tokens.is_empty() {
        normalize_subject_tokens(&constraint.subject)
    } else {
        constraint.related_subject_tokens.clone()
    }
}

pub(crate) fn merge_constraint_evidence(
    existing: &NegativeConstraint,
    incoming: &NegativeConstraint,
) -> NegativeConstraint {
    let mut related_subject_tokens = effective_related_subject_tokens(existing);
    related_subject_tokens.extend(effective_related_subject_tokens(incoming));
    related_subject_tokens.sort();
    related_subject_tokens.dedup();

    let direct_observation = existing.direct_observation || incoming.direct_observation;
    let confidence = existing.confidence.max(incoming.confidence);
    let evidence_count = existing.evidence_count.saturating_add(1);

    NegativeConstraint {
        id: existing.id.clone(),
        episode_id: incoming
            .episode_id
            .clone()
            .or_else(|| existing.episode_id.clone()),
        constraint_type: existing.constraint_type,
        subject: existing.subject.clone(),
        solution_class: existing.solution_class.clone(),
        description: incoming.description.clone(),
        confidence,
        state: next_constraint_state(
            existing.state,
            evidence_count,
            direct_observation,
            confidence,
        ),
        evidence_count,
        direct_observation,
        derived_from_constraint_ids: existing.derived_from_constraint_ids.clone(),
        related_subject_tokens,
        valid_until: incoming.valid_until.or(existing.valid_until),
        created_at: existing.created_at,
    }
}

pub(crate) fn propagate_dead_constraint(
    source: &NegativeConstraint,
    constraints: &[NegativeConstraint],
) -> Vec<NegativeConstraint> {
    if source.state != ConstraintState::Dead {
        return Vec::new();
    }

    let mut candidates: Vec<NegativeConstraint> = constraints
        .iter()
        .filter(|target| target.id != source.id)
        .filter(|target| target.state != ConstraintState::Dead)
        .filter(|target| related_for_propagation(source, target))
        .cloned()
        .collect();

    candidates.sort_by(|a, b| {
        constraint_state_rank(a.state)
            .cmp(&constraint_state_rank(b.state))
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    candidates
        .into_iter()
        .take(10)
        .map(|mut target| {
            if !target
                .derived_from_constraint_ids
                .iter()
                .any(|existing_id| existing_id == &source.id)
            {
                target.derived_from_constraint_ids.push(source.id.clone());
            }

            if target.state == ConstraintState::Suspicious {
                target.state = ConstraintState::Dying;
            }

            if !target.direct_observation {
                target.direct_observation = false;
            }

            target
        })
        .collect()
}

/// Check if a constraint is still active (not expired).
pub fn is_constraint_active(constraint: &NegativeConstraint, now_ms: u64) -> bool {
    match constraint.valid_until {
        None => true,
        Some(expiry) => expiry > now_ms,
    }
}

fn constraint_state_rank(state: ConstraintState) -> u8 {
    match state {
        ConstraintState::Dead => 3,
        ConstraintState::Dying => 2,
        ConstraintState::Suspicious => 1,
    }
}

fn constraint_state_label(state: ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "DO NOT attempt",
        ConstraintState::Dying => "Avoid unless you have new evidence",
        ConstraintState::Suspicious => "Use caution",
    }
}

fn constraint_state_str(state: ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "dead",
        ConstraintState::Dying => "dying",
        ConstraintState::Suspicious => "suspicious",
    }
}

fn constraint_state_to_str(state: &ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "dead",
        ConstraintState::Dying => "dying",
        ConstraintState::Suspicious => "suspicious",
    }
}

fn str_to_constraint_state(s: &str) -> ConstraintState {
    match s {
        "dead" => ConstraintState::Dead,
        "suspicious" => ConstraintState::Suspicious,
        _ => ConstraintState::Dying,
    }
}

fn parse_json_string_vec(value: String) -> Vec<String> {
    if value.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&value).unwrap_or_default()
    }
}

fn constraint_source_line(constraint: &NegativeConstraint) -> String {
    let source = if constraint.direct_observation {
        "direct"
    } else {
        "inferred"
    };

    if constraint.derived_from_constraint_ids.is_empty() {
        format!("Source: {source}")
    } else {
        let count = constraint.derived_from_constraint_ids.len();
        let noun = if count == 1 {
            "constraint"
        } else {
            "constraints"
        };
        format!("Source: {source} from {count} related dead {noun}")
    }
}

/// Format active negative constraints for system prompt injection.
/// Filters to active only, sorts strongest-first, and caps at 10.
pub fn format_negative_constraints(constraints: &[NegativeConstraint], now_ms: u64) -> String {
    let mut active: Vec<&NegativeConstraint> = constraints
        .iter()
        .filter(|c| is_constraint_active(c, now_ms))
        .collect();

    if active.is_empty() {
        return String::new();
    }

    active.sort_by(|a, b| {
        constraint_state_rank(b.state)
            .cmp(&constraint_state_rank(a.state))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    let mut out = String::new();
    out.push_str("## Ruled-Out Approaches (Negative Knowledge)\n");

    let display_count = active.len().min(10);
    for constraint in active.iter().take(display_count) {
        let constraint_type_str = match constraint.constraint_type {
            ConstraintType::RuledOut => "ruled_out",
            ConstraintType::ImpossibleCombination => "impossible_combination",
            ConstraintType::KnownLimitation => "known_limitation",
        };

        out.push_str(&format!(
            "{}: {}\n",
            constraint_state_label(constraint.state),
            constraint.subject
        ));
        out.push_str(&format!(
            "  State: {}\n",
            constraint_state_str(constraint.state)
        ));
        out.push_str(&format!("  Reason: {}\n", constraint.description));
        out.push_str(&format!(
            "  Type: {} (confidence: {:.0}%)\n",
            constraint_type_str,
            constraint.confidence * 100.0
        ));
        out.push_str(&format!("  {}\n", constraint_source_line(constraint)));

        if let Some(ref sc) = constraint.solution_class {
            out.push_str(&format!("  Solution class: {sc}\n"));
        }

        match constraint.valid_until {
            Some(expiry) => {
                // Format as human-readable date
                let days_remaining = expiry.saturating_sub(now_ms) / (86400 * 1000);
                out.push_str(&format!("  Expires: in {days_remaining} days\n"));
            }
            None => {
                out.push_str("  Expires: never\n");
            }
        }

        out.push('\n');
    }

    if active.len() > 10 {
        let remaining = active.len() - 10;
        out.push_str(&format!("({remaining} more constraints not shown)\n"));
    }

    out
}

// ---------------------------------------------------------------------------
// AgentEngine integration methods
// ---------------------------------------------------------------------------

fn constraint_type_to_str(ct: &ConstraintType) -> &'static str {
    match ct {
        ConstraintType::RuledOut => "ruled_out",
        ConstraintType::ImpossibleCombination => "impossible_combination",
        ConstraintType::KnownLimitation => "known_limitation",
    }
}

fn str_to_constraint_type(s: &str) -> ConstraintType {
    match s {
        "impossible_combination" => ConstraintType::ImpossibleCombination,
        "known_limitation" => ConstraintType::KnownLimitation,
        _ => ConstraintType::RuledOut,
    }
}

fn load_all_active_constraints(
    conn: &rusqlite::Connection,
    agent_id: &str,
    include_legacy: i64,
    now_ms: i64,
) -> rusqlite::Result<Vec<NegativeConstraint>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                description, confidence, state, evidence_count, direct_observation,
                derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
         FROM negative_knowledge
         WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
           AND (valid_until IS NULL OR valid_until > ?3)
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![agent_id, include_legacy, now_ms], row_to_constraint)?;
    let mut constraints = Vec::new();
    for row in rows {
        constraints.push(row?);
    }
    Ok(constraints)
}

fn persist_constraint(
    conn: &rusqlite::Connection,
    constraint: &NegativeConstraint,
    agent_id: &str,
) -> rusqlite::Result<()> {
    let derived_from_constraint_ids =
        serde_json::to_string(&constraint.derived_from_constraint_ids)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    let related_subject_tokens = serde_json::to_string(&constraint.related_subject_tokens)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;

    conn.execute(
        "INSERT OR REPLACE INTO negative_knowledge
         (id, agent_id, episode_id, constraint_type, subject, solution_class,
          description, confidence, state, evidence_count, direct_observation,
          derived_from_constraint_ids, related_subject_tokens, valid_until, created_at)
          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            constraint.id,
            agent_id,
            constraint.episode_id,
            constraint_type_to_str(&constraint.constraint_type),
            constraint.subject,
            constraint.solution_class,
            constraint.description,
            constraint.confidence,
            constraint_state_to_str(&constraint.state),
            constraint.evidence_count,
            if constraint.direct_observation { 1 } else { 0 },
            derived_from_constraint_ids,
            related_subject_tokens,
            constraint.valid_until.map(|v| v as i64),
            constraint.created_at as i64,
        ],
    )?;

    Ok(())
}

fn persist_constraint_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    constraint: &NegativeConstraint,
    agent_id: &str,
) -> rusqlite::Result<()> {
    let derived_from_constraint_ids =
        serde_json::to_string(&constraint.derived_from_constraint_ids)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    let related_subject_tokens = serde_json::to_string(&constraint.related_subject_tokens)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;

    tx.execute(
        "INSERT OR REPLACE INTO negative_knowledge
         (id, agent_id, episode_id, constraint_type, subject, solution_class,
          description, confidence, state, evidence_count, direct_observation,
          derived_from_constraint_ids, related_subject_tokens, valid_until, created_at)
          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            constraint.id,
            agent_id,
            constraint.episode_id,
            constraint_type_to_str(&constraint.constraint_type),
            constraint.subject,
            constraint.solution_class,
            constraint.description,
            constraint.confidence,
            constraint_state_to_str(&constraint.state),
            constraint.evidence_count,
            if constraint.direct_observation { 1 } else { 0 },
            derived_from_constraint_ids,
            related_subject_tokens,
            constraint.valid_until.map(|v| v as i64),
            constraint.created_at as i64,
        ],
    )?;

    Ok(())
}

impl AgentEngine {
    /// Add a negative knowledge constraint (NKNO-01).
    pub(crate) async fn add_negative_constraint(
        &self,
        constraint: NegativeConstraint,
    ) -> Result<()> {
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let now_ms = super::super::now_millis();
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

    /// Record negative knowledge from a failed episode (NKNO-02).
    /// Only creates a constraint when the episode is a failure with a root cause.
    pub(crate) async fn record_negative_knowledge_from_episode(
        &self,
        episode: &Episode,
    ) -> Result<()> {
        // Only process failures with root causes
        if episode.outcome != EpisodeOutcome::Failure || episode.root_cause.is_none() {
            return Ok(());
        }

        let config = self.config.read().await;
        let constraint_ttl_days = config.episodic.constraint_ttl_days;
        drop(config);

        let now_ms = super::super::now_millis();
        let valid_until = now_ms + constraint_ttl_days * 86400 * 1000;

        let constraint = build_direct_constraint_from_episode(
            episode,
            now_ms,
            valid_until,
            format!("nc_{}", uuid::Uuid::new_v4()),
        );

        self.add_negative_constraint(constraint).await
    }

    /// Query active (non-expired) constraints, optionally filtered by entity (NKNO-03).
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
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows =
                        stmt.query_map(params![agent_id, include_legacy, now_ms, pattern], row_to_constraint)?;
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
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows = stmt.query_map(params![agent_id, include_legacy, now_ms], row_to_constraint)?;
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

    /// Expire (delete) constraints past their TTL (NKNO-04).
    pub(crate) async fn expire_negative_constraints(&self) -> Result<usize> {
        let now_ms = crate::agent::now_millis() as i64;

        let deleted = self
            .history
            .conn
            .call(move |conn| {
                let count = conn.execute(
                    "DELETE FROM negative_knowledge WHERE valid_until IS NOT NULL AND valid_until <= ?1",
                    params![now_ms],
                )?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Update cached constraints: filter out expired
        if deleted > 0 {
            let now_ms = super::super::now_millis();
            let scope_id = crate::agent::agent_identity::current_agent_scope_id();
            let mut stores = self.episodic_store.write().await;
            let store = stores.entry(scope_id).or_default();
            store
                .cached_constraints
                .retain(|c| is_constraint_active(c, now_ms));
        }

        Ok(deleted)
    }

    /// Refresh the in-memory constraint cache from the database.
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

fn row_to_constraint(row: &rusqlite::Row<'_>) -> rusqlite::Result<NegativeConstraint> {
    let ct_str: String = row.get(3)?;
    let state_str: String = row.get(8)?;
    let direct_observation: i64 = row.get(10)?;
    let derived_from_constraint_ids = parse_json_string_vec(row.get(11)?);
    let related_subject_tokens = parse_json_string_vec(row.get(12)?);

    Ok(NegativeConstraint {
        id: row.get(0)?,
        episode_id: row.get(2)?,
        constraint_type: str_to_constraint_type(&ct_str),
        subject: row.get(4)?,
        solution_class: row.get(5)?,
        description: row.get(6)?,
        confidence: row.get(7)?,
        state: str_to_constraint_state(&state_str),
        evidence_count: row.get(9)?,
        direct_observation: direct_observation != 0,
        derived_from_constraint_ids,
        related_subject_tokens,
        valid_until: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
        created_at: row.get::<_, i64>(14)? as u64,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::schema::init_episodic_schema;
    use super::*;
    use crate::agent::{types::AgentConfig, SessionManager};
    use rusqlite::{params, Connection};
    use tempfile::tempdir;

    fn make_constraint(subject: &str, valid_until: Option<u64>) -> NegativeConstraint {
        NegativeConstraint {
            id: format!("nc-{subject}"),
            episode_id: Some("ep-001".to_string()),
            constraint_type: ConstraintType::RuledOut,
            subject: subject.to_string(),
            solution_class: Some("test-class".to_string()),
            description: format!("Reason for {subject}"),
            confidence: 0.85,
            state: ConstraintState::Dying,
            evidence_count: 1,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: Vec::new(),
            valid_until,
            created_at: 1_000_000_000,
        }
    }

    fn make_constraint_with_class(
        subject: &str,
        solution_class: Option<&str>,
    ) -> NegativeConstraint {
        NegativeConstraint {
            solution_class: solution_class.map(str::to_string),
            ..make_constraint(subject, Some(2_000_000_000))
        }
    }

    fn make_constraint_with_details(
        subject: &str,
        state: ConstraintState,
        created_at: u64,
        direct_observation: bool,
        derived_from_constraint_ids: &[&str],
    ) -> NegativeConstraint {
        NegativeConstraint {
            subject: subject.to_string(),
            state,
            created_at,
            direct_observation,
            derived_from_constraint_ids: derived_from_constraint_ids
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
            valid_until: Some(2_000_000_000),
            ..make_constraint(subject, Some(2_000_000_000))
        }
    }

    fn make_failure_episode(summary: &str, confidence: Option<f64>) -> Episode {
        Episode {
            id: "ep-failure".to_string(),
            goal_run_id: None,
            thread_id: None,
            session_id: None,
            goal_text: None,
            goal_type: None,
            episode_type: super::super::EpisodeType::GoalFailure,
            summary: summary.to_string(),
            outcome: EpisodeOutcome::Failure,
            root_cause: Some("root cause".to_string()),
            entities: Vec::new(),
            causal_chain: Vec::new(),
            solution_class: Some("test-class".to_string()),
            duration_ms: None,
            tokens_used: None,
            confidence,
            confidence_before: None,
            confidence_after: None,
            created_at: 1_000,
            expires_at: None,
        }
    }

    fn select_constraint_by_id(
        conn: &Connection,
        id: &str,
    ) -> rusqlite::Result<NegativeConstraint> {
        conn.query_row(
            "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                    description, confidence, state, evidence_count, direct_observation,
                    derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
             FROM negative_knowledge
             WHERE id = ?1",
            params![id],
            row_to_constraint,
        )
    }

    async fn make_test_engine() -> std::sync::Arc<AgentEngine> {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await
    }

    async fn insert_constraint_for_engine(
        engine: &AgentEngine,
        constraint: NegativeConstraint,
    ) -> anyhow::Result<()> {
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        engine
            .history
            .conn
            .call(move |conn| {
                persist_constraint(conn, &constraint, &agent_id)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    async fn select_constraints_for_subject(
        engine: &AgentEngine,
        subject: &str,
    ) -> anyhow::Result<Vec<NegativeConstraint>> {
        let subject = subject.to_string();
        engine
            .history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                            description, confidence, state, evidence_count, direct_observation,
                            derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
                     FROM negative_knowledge
                     WHERE subject = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![subject], row_to_constraint)?;
                let mut constraints = Vec::new();
                for row in rows {
                    constraints.push(row?);
                }
                Ok(constraints)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn count_negative_knowledge_rows(engine: &AgentEngine) -> anyhow::Result<u32> {
        engine
            .history
            .conn
            .call(|conn| {
                let count: u32 =
                    conn.query_row("SELECT COUNT(*) FROM negative_knowledge", [], |row| {
                        row.get(0)
                    })?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    #[test]
    fn row_to_constraint_reads_richer_persisted_fields() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        init_episodic_schema(&conn)?;

        conn.execute(
            "INSERT INTO negative_knowledge
             (id, agent_id, episode_id, constraint_type, subject, solution_class,
              description, confidence, state, evidence_count, direct_observation,
              derived_from_constraint_ids, related_subject_tokens, valid_until, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                "nc-rich",
                "agent-1",
                "ep-123",
                "known_limitation",
                "rich subject",
                "solver",
                "cannot proceed",
                0.92,
                "dead",
                4,
                0,
                "[\"nc-a\",\"nc-b\"]",
                "[\"alpha\",\"beta\"]",
                1_234_567i64,
                7_654_321i64,
            ],
        )?;

        let constraint = select_constraint_by_id(&conn, "nc-rich")?;

        assert_eq!(constraint.state, ConstraintState::Dead);
        assert_eq!(constraint.evidence_count, 4);
        assert!(!constraint.direct_observation);
        assert_eq!(
            constraint.derived_from_constraint_ids,
            vec!["nc-a".to_string(), "nc-b".to_string()]
        );
        assert_eq!(
            constraint.related_subject_tokens,
            vec!["alpha".to_string(), "beta".to_string()]
        );

        Ok(())
    }

    #[test]
    fn row_to_constraint_defaults_new_fields_for_legacy_rows() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        init_episodic_schema(&conn)?;

        conn.execute(
            "INSERT INTO negative_knowledge
             (id, agent_id, episode_id, constraint_type, subject, solution_class,
              description, confidence, valid_until, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                "nc-legacy",
                "agent-1",
                "ep-legacy",
                "ruled_out",
                "legacy subject",
                "solver",
                "legacy description",
                0.61,
                2_222_222i64,
                3_333_333i64,
            ],
        )?;

        let constraint = select_constraint_by_id(&conn, "nc-legacy")?;

        assert_eq!(constraint.state, ConstraintState::Dying);
        assert_eq!(constraint.evidence_count, 1);
        assert!(constraint.direct_observation);
        assert!(constraint.derived_from_constraint_ids.is_empty());
        assert!(constraint.related_subject_tokens.is_empty());

        Ok(())
    }

    #[test]
    fn format_negative_constraints_empty_returns_empty() {
        assert!(format_negative_constraints(&[], 1_000_000_000).is_empty());
    }

    #[test]
    fn format_negative_constraints_groups_and_sorts_by_state_then_created_at() {
        let constraints = vec![
            make_constraint_with_details(
                "suspicious old",
                ConstraintState::Suspicious,
                100,
                true,
                &[],
            ),
            make_constraint_with_details("dead newer", ConstraintState::Dead, 300, true, &["nc-1"]),
            make_constraint_with_details(
                "dying newest",
                ConstraintState::Dying,
                400,
                false,
                &["nc-2"],
            ),
            make_constraint_with_details("dead oldest", ConstraintState::Dead, 200, true, &[]),
            make_constraint_with_details(
                "suspicious newer",
                ConstraintState::Suspicious,
                500,
                false,
                &[],
            ),
        ];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        let dead_newer = result.find("DO NOT attempt: dead newer").unwrap();
        let dead_oldest = result.find("DO NOT attempt: dead oldest").unwrap();
        let dying_newest = result
            .find("Avoid unless you have new evidence: dying newest")
            .unwrap();
        let suspicious_newer = result.find("Use caution: suspicious newer").unwrap();
        let suspicious_old = result.find("Use caution: suspicious old").unwrap();

        assert!(dead_newer < dead_oldest);
        assert!(dead_oldest < dying_newest);
        assert!(dying_newest < suspicious_newer);
        assert!(suspicious_newer < suspicious_old);
    }

    #[test]
    fn format_negative_constraints_renders_state_metadata_and_conditional_provenance() {
        let constraints = vec![
            make_constraint_with_details("dead path", ConstraintState::Dead, 300, true, &["nc-1"]),
            make_constraint_with_details(
                "dying path",
                ConstraintState::Dying,
                200,
                false,
                &["nc-2"],
            ),
            make_constraint_with_details(
                "suspicious path",
                ConstraintState::Suspicious,
                100,
                true,
                &[],
            ),
        ];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        assert!(result.starts_with("## Ruled-Out Approaches (Negative Knowledge)\n"));
        assert!(result.contains("DO NOT attempt: dead path"));
        assert!(result.contains("Avoid unless you have new evidence: dying path"));
        assert!(result.contains("Use caution: suspicious path"));
        assert!(result.contains("State: dead"));
        assert!(result.contains("State: dying"));
        assert!(result.contains("State: suspicious"));
        assert!(result.contains("Reason: Reason for dead path"));
        assert!(result.contains("Type: ruled_out"));
        assert!(result.contains("confidence: 85%"));
        assert!(result.contains("Source: direct"));
        assert!(result.contains("Source: inferred"));
        assert!(result.contains("Source: direct from 1 related dead constraint"));
        assert!(result.contains("Source: inferred from 1 related dead constraint"));
        assert!(result.contains("\n  Source: direct\n  Solution class: test-class\n"));
        assert!(!result.contains("Source: direct from 0 related dead constraints"));
    }

    #[test]
    fn format_negative_constraints_excludes_expired_constraints() {
        let constraints = vec![
            make_constraint_with_details("active path", ConstraintState::Dead, 300, true, &[]),
            make_constraint("expired path", Some(999_999_999)),
        ];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        assert!(result.contains("DO NOT attempt: active path"));
        assert!(!result.contains("expired path"));
    }

    #[test]
    fn format_negative_constraints_caps_display_at_ten_and_shows_overflow_count() {
        let constraints: Vec<NegativeConstraint> = (0..11)
            .map(|idx| {
                make_constraint_with_details(
                    &format!("constraint {idx}"),
                    ConstraintState::Dead,
                    1_000 + idx,
                    true,
                    &[],
                )
            })
            .collect();

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        assert!(result.contains("DO NOT attempt: constraint 10"));
        assert!(result.contains("DO NOT attempt: constraint 1"));
        assert!(!result.contains("DO NOT attempt: constraint 0"));
        assert!(result.contains("(1 more constraints not shown)"));
    }

    #[test]
    fn format_negative_constraints_renders_exact_inferred_source_without_provenance() {
        let constraints = vec![make_constraint_with_details(
            "inferred path",
            ConstraintState::Suspicious,
            100,
            false,
            &[],
        )];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        assert!(result.contains("\n  Source: inferred\n  Solution class: test-class\n"));
        assert!(!result.contains("Source: inferred from"));
    }

    #[test]
    fn format_negative_constraints_with_two_constraints() {
        let constraints = vec![
            make_constraint("npm install approach", Some(2_000_000_000)),
            make_constraint("yarn install approach", None),
        ];
        let result = format_negative_constraints(&constraints, 1_000_000_000);
        assert!(result.contains("Avoid unless you have new evidence: npm install approach"));
        assert!(result.contains("Avoid unless you have new evidence: yarn install approach"));
        assert!(result.contains("Ruled-Out Approaches"));
        assert!(result.contains("State: dying"));
        assert!(result.contains("Reason: Reason for npm install approach"));
        assert!(result.contains("Type: ruled_out"));
        assert!(result.contains("confidence: 85%"));
        assert!(result.contains("Source: direct"));
    }

    #[test]
    fn format_negative_constraints_includes_solution_class() {
        let constraints = vec![make_constraint("bad approach", Some(2_000_000_000))];
        let result = format_negative_constraints(&constraints, 1_000_000_000);
        assert!(result.contains("Solution class: test-class"));
    }

    #[test]
    fn format_negative_constraints_includes_expiry() {
        let now_ms = 1_000_000_000u64;
        let in_10_days = now_ms + 10 * 86400 * 1000;
        let constraints = vec![make_constraint("approach", Some(in_10_days))];
        let result = format_negative_constraints(&constraints, now_ms);
        assert!(result.contains("Expires: in 10 days"));
    }

    #[test]
    fn is_constraint_active_no_valid_until_returns_true() {
        let c = make_constraint("test", None);
        assert!(is_constraint_active(&c, 9_999_999_999));
    }

    #[test]
    fn is_constraint_active_future_valid_until_returns_true() {
        let c = make_constraint("test", Some(2_000_000_000));
        assert!(is_constraint_active(&c, 1_000_000_000));
    }

    #[test]
    fn is_constraint_active_past_valid_until_returns_false() {
        let c = make_constraint("test", Some(500_000_000));
        assert!(!is_constraint_active(&c, 1_000_000_000));
    }

    #[test]
    fn normalize_subject_tokens_sorts_dedupes_and_filters() {
        assert_eq!(
            normalize_subject_tokens("Fix deploy-config in prod!"),
            vec!["config", "deploy", "fix", "prod"]
        );
    }

    #[test]
    fn normalize_subject_tokens_is_stable_across_case_and_punctuation() {
        assert_eq!(
            normalize_subject_tokens("Deploy, CONFIG; fix fix prod??"),
            vec!["config", "deploy", "fix", "prod"]
        );
    }

    #[test]
    fn normalized_subject_key_returns_stable_deduped_key() {
        assert_eq!(
            normalized_subject_key("Fix deploy-config in prod!"),
            "config deploy fix prod"
        );
        assert_eq!(
            normalized_subject_key("prod deploy fix config fix"),
            "config deploy fix prod"
        );
    }

    #[test]
    fn constraints_match_for_merge_requires_same_normalized_subject_and_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", Some("deploy-fix"));

        assert!(constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_different_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", Some("ops-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_missing_solution_class_on_one_side() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", None);

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_allows_matching_none_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", None);
        let b = make_constraint_with_class("prod deploy fix config", None);

        assert!(constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_empty_normalized_subjects() {
        let a = make_constraint_with_class("CI CD", Some("deploy-fix"));
        let b = make_constraint_with_class("QA DB", Some("deploy-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_different_normalized_subjects_with_same_solution_class()
    {
        let a = make_constraint_with_class("deploy config rollback", Some("deploy-fix"));
        let b = make_constraint_with_class("cache rebuild timeout", Some("deploy-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn related_for_propagation_requires_two_shared_tokens_with_same_solution_class() {
        let source = make_constraint_with_class("fix deploy config prod", Some("deploy-fix"));
        let target = make_constraint_with_class("deploy config rollback", Some("deploy-fix"));

        assert!(related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_rejects_same_class_with_only_one_shared_token() {
        let source = make_constraint_with_class("fix deploy config prod", Some("deploy-fix"));
        let target = make_constraint_with_class("deploy cache rebuild", Some("deploy-fix"));

        assert!(!related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_requires_three_shared_tokens_without_solution_class() {
        let source = make_constraint_with_class("deploy config prod fix", None);
        let target = make_constraint_with_class("prod deploy config rollback", None);

        assert!(related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_rejects_mixed_solution_class_even_with_shared_tokens() {
        let source = make_constraint_with_class("deploy config prod fix", Some("deploy-fix"));
        let target = make_constraint_with_class("prod deploy config rollback", None);

        assert!(!related_for_propagation(&source, &target));
    }

    #[test]
    fn next_constraint_state_keeps_dead_dead() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dead, 1, false, 0.2),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dead_for_direct_high_confidence_observation() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, true, 0.85),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dead_at_three_evidence() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dying, 3, false, 0.4),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dying_at_two_evidence() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 2, false, 0.4),
            ConstraintState::Dying
        );
    }

    #[test]
    fn next_constraint_state_does_not_promote_for_direct_observation_alone() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, true, 0.84),
            ConstraintState::Suspicious
        );
    }

    #[test]
    fn next_constraint_state_keeps_existing_non_terminal_state_when_thresholds_not_met() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dying, 1, false, 0.4),
            ConstraintState::Dying
        );
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, false, 0.4),
            ConstraintState::Suspicious
        );
    }

    #[test]
    fn propagation_direct_failure_derived_constraint_defaults_to_dying() {
        let episode = make_failure_episode("deploy config rollback failed", Some(0.7));

        let constraint =
            build_direct_constraint_from_episode(&episode, 10_000, 20_000, "nc-new".to_string());

        assert_eq!(constraint.state, ConstraintState::Dying);
        assert_eq!(constraint.evidence_count, 1);
        assert!(constraint.direct_observation);
    }

    #[test]
    fn propagation_high_confidence_direct_evidence_yields_dead() {
        let episode = make_failure_episode("deploy config rollback failed", Some(0.93));

        let constraint =
            build_direct_constraint_from_episode(&episode, 10_000, 20_000, "nc-new".to_string());

        assert_eq!(constraint.state, ConstraintState::Dead);
        assert!(constraint.direct_observation);
    }

    #[tokio::test]
    async fn record_negative_knowledge_from_episode_initializes_failure_derived_constraint_fields(
    ) -> anyhow::Result<()> {
        let engine = make_test_engine().await;
        let episode = make_failure_episode("Deploy CONFIG rollback failed", Some(0.84));

        engine
            .record_negative_knowledge_from_episode(&episode)
            .await?;

        let constraints =
            select_constraints_for_subject(&engine, "Deploy CONFIG rollback failed").await?;

        assert_eq!(constraints.len(), 1);
        assert_eq!(
            constraints[0].related_subject_tokens,
            vec!["config", "deploy", "failed", "rollback"]
        );
        assert!(constraints[0].direct_observation);
        assert_eq!(constraints[0].state, ConstraintState::Dying);
        assert_eq!(constraints[0].evidence_count, 1);
        assert!(constraints[0].derived_from_constraint_ids.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn record_negative_knowledge_from_episode_promotes_high_confidence_failure_to_dead(
    ) -> anyhow::Result<()> {
        let engine = make_test_engine().await;
        let episode = make_failure_episode("Deploy CONFIG rollback failed", Some(0.85));

        engine
            .record_negative_knowledge_from_episode(&episode)
            .await?;

        let constraints =
            select_constraints_for_subject(&engine, "Deploy CONFIG rollback failed").await?;

        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].state, ConstraintState::Dead);
        assert!(constraints[0].direct_observation);

        Ok(())
    }

    #[test]
    fn propagation_merge_matching_evidence_upgrades_suspicious_to_dying() {
        let existing = NegativeConstraint {
            state: ConstraintState::Suspicious,
            evidence_count: 1,
            confidence: 0.4,
            direct_observation: false,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };
        let incoming = NegativeConstraint {
            confidence: 0.7,
            ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
        };

        let merged = merge_constraint_evidence(&existing, &incoming);

        assert_eq!(merged.state, ConstraintState::Dying);
        assert_eq!(merged.evidence_count, 2);
    }

    #[test]
    fn propagation_repeated_matching_evidence_upgrades_to_dead_at_three() {
        let existing = NegativeConstraint {
            state: ConstraintState::Dying,
            evidence_count: 2,
            confidence: 0.7,
            direct_observation: true,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };
        let incoming = NegativeConstraint {
            confidence: 0.72,
            ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
        };

        let merged = merge_constraint_evidence(&existing, &incoming);

        assert_eq!(merged.state, ConstraintState::Dead);
        assert_eq!(merged.evidence_count, 3);
    }

    #[test]
    fn propagation_dead_source_upgrades_related_suspicious_targets_to_dying() {
        let source = NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dead,
            ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
        };
        let target = NegativeConstraint {
            id: "nc-target".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };

        let propagated = propagate_dead_constraint(&source, &[source.clone(), target]);

        assert_eq!(propagated.len(), 1);
        assert_eq!(propagated[0].id, "nc-target");
        assert_eq!(propagated[0].state, ConstraintState::Dying);
    }

    #[test]
    fn propagation_appends_source_id_to_derived_from_constraint_ids() {
        let source = NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dead,
            ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
        };
        let target = NegativeConstraint {
            id: "nc-target".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            derived_from_constraint_ids: vec!["nc-other".to_string()],
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };

        let propagated = propagate_dead_constraint(&source, &[source.clone(), target]);

        assert_eq!(
            propagated[0].derived_from_constraint_ids,
            vec!["nc-other".to_string(), "nc-source".to_string()]
        );
    }

    #[test]
    fn propagation_does_not_overwrite_direct_observation_true() {
        let source = NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dead,
            ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
        };
        let target = NegativeConstraint {
            id: "nc-target".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: true,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };

        let propagated = propagate_dead_constraint(&source, &[source.clone(), target]);

        assert!(propagated[0].direct_observation);
    }

    #[test]
    fn propagation_sets_direct_observation_false_only_for_targets_without_direct_evidence() {
        let source = NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dead,
            ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
        };
        let inferred_target = NegativeConstraint {
            id: "nc-inferred".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            created_at: 200,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };
        let direct_target = NegativeConstraint {
            id: "nc-direct".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: true,
            created_at: 100,
            ..make_constraint_with_class("deploy config fallback", Some("deploy-fix"))
        };

        let propagated =
            propagate_dead_constraint(&source, &[source.clone(), inferred_target, direct_target]);

        assert_eq!(propagated.len(), 2);
        assert!(
            !propagated
                .iter()
                .find(|c| c.id == "nc-inferred")
                .unwrap()
                .direct_observation
        );
        assert!(
            propagated
                .iter()
                .find(|c| c.id == "nc-direct")
                .unwrap()
                .direct_observation
        );
    }

    #[test]
    fn propagation_is_capped_at_ten_targets_and_does_not_recurse() {
        let source = NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dead,
            ..make_constraint_with_class("alpha beta gamma root", Some("deploy-fix"))
        };
        let mut constraints = vec![source.clone()];

        for idx in 0..11 {
            constraints.push(NegativeConstraint {
                id: format!("nc-related-{idx}"),
                state: ConstraintState::Suspicious,
                direct_observation: false,
                created_at: 100 + idx,
                ..make_constraint_with_class(
                    &format!("alpha beta related {idx}"),
                    Some("deploy-fix"),
                )
            });
        }

        constraints.push(NegativeConstraint {
            id: "nc-second-hop".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            created_at: 10_000,
            subject: "alpha related branch leaf".to_string(),
            ..make_constraint_with_class("alpha related branch leaf", Some("deploy-fix"))
        });

        let propagated = propagate_dead_constraint(&source, &constraints);
        let propagated_ids: Vec<&str> = propagated
            .iter()
            .map(|constraint| constraint.id.as_str())
            .collect();

        assert_eq!(propagated.len(), 10);
        assert!(propagated
            .iter()
            .all(|constraint| constraint.state == ConstraintState::Dying));
        assert!(!propagated_ids.contains(&"nc-related-0"));
        assert!(propagated_ids.contains(&"nc-related-10"));
        assert!(!propagated_ids.contains(&"nc-second-hop"));
    }

    #[tokio::test]
    async fn add_negative_constraint_merges_with_matching_row_beyond_twenty_row_fallback(
    ) -> anyhow::Result<()> {
        let engine = make_test_engine().await;

        let old_match = NegativeConstraint {
            id: "nc-old-match".to_string(),
            state: ConstraintState::Suspicious,
            confidence: 0.4,
            evidence_count: 1,
            direct_observation: false,
            valid_until: None,
            created_at: 10,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        };
        insert_constraint_for_engine(&engine, old_match).await?;

        for idx in 0..24 {
            insert_constraint_for_engine(
                &engine,
                NegativeConstraint {
                    id: format!("nc-filler-{idx}"),
                    created_at: 1_000 + idx,
                    subject: format!("filler subject {idx}"),
                    valid_until: None,
                    ..make_constraint_with_class(
                        &format!("filler subject {idx}"),
                        Some("deploy-fix"),
                    )
                },
            )
            .await?;
        }

        engine
            .add_negative_constraint(NegativeConstraint {
                id: "nc-incoming".to_string(),
                confidence: 0.7,
                valid_until: None,
                ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
            })
            .await?;

        let matching = select_constraints_for_subject(&engine, "deploy config rollback").await?;
        let row_count = count_negative_knowledge_rows(&engine).await?;

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "nc-old-match");
        assert_eq!(matching[0].evidence_count, 2);
        assert_eq!(matching[0].state, ConstraintState::Dying);
        assert_eq!(row_count, 25);

        Ok(())
    }

    #[tokio::test]
    async fn add_negative_constraint_persists_source_and_propagated_target_updates_together(
    ) -> anyhow::Result<()> {
        let engine = make_test_engine().await;

        insert_constraint_for_engine(
            &engine,
            NegativeConstraint {
                id: "nc-source".to_string(),
                state: ConstraintState::Dying,
                evidence_count: 2,
                confidence: 0.72,
                valid_until: None,
                created_at: 100,
                ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
            },
        )
        .await?;

        insert_constraint_for_engine(
            &engine,
            NegativeConstraint {
                id: "nc-target".to_string(),
                state: ConstraintState::Suspicious,
                direct_observation: false,
                valid_until: None,
                created_at: 90,
                ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
            },
        )
        .await?;

        engine
            .add_negative_constraint(NegativeConstraint {
                id: "nc-source-incoming".to_string(),
                confidence: 0.8,
                valid_until: None,
                ..make_constraint_with_class("fix deploy config prod", Some("deploy-fix"))
            })
            .await?;

        let source = engine
            .history
            .conn
            .call(|conn| Ok(select_constraint_by_id(conn, "nc-source")?))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let target = engine
            .history
            .conn
            .call(|conn| Ok(select_constraint_by_id(conn, "nc-target")?))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        assert_eq!(source.state, ConstraintState::Dead);
        assert_eq!(source.evidence_count, 3);
        assert_eq!(target.state, ConstraintState::Dying);
        assert_eq!(
            target.derived_from_constraint_ids,
            vec!["nc-source".to_string()]
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_negative_constraint_cache_update_preserves_concurrent_entries(
    ) -> anyhow::Result<()> {
        let engine = make_test_engine().await;
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();

        {
            let mut stores = engine.episodic_store.write().await;
            let store = stores.entry(scope_id.clone()).or_default();
            store.cached_constraints.push(NegativeConstraint {
                id: "nc-concurrent-only".to_string(),
                created_at: 999,
                valid_until: None,
                ..make_constraint_with_class("concurrent cache only", Some("deploy-fix"))
            });
        }

        engine
            .add_negative_constraint(NegativeConstraint {
                id: "nc-new-cache".to_string(),
                created_at: 1_001,
                valid_until: None,
                ..make_constraint_with_class("fresh runtime insert", Some("deploy-fix"))
            })
            .await?;

        let stores = engine.episodic_store.read().await;
        let store = stores.get(&scope_id).expect("store exists");

        assert!(store
            .cached_constraints
            .iter()
            .any(|constraint| constraint.id == "nc-concurrent-only"));
        assert!(store
            .cached_constraints
            .iter()
            .any(|constraint| constraint.id == "nc-new-cache"));

        Ok(())
    }

    #[tokio::test]
    async fn refresh_constraint_cache_can_load_more_than_twenty_active_rows() -> anyhow::Result<()>
    {
        let engine = make_test_engine().await;

        for idx in 0..25 {
            insert_constraint_for_engine(
                &engine,
                NegativeConstraint {
                    id: format!("nc-refresh-{idx}"),
                    created_at: 5_000 + idx,
                    subject: format!("refresh subject {idx}"),
                    valid_until: None,
                    ..make_constraint_with_class(
                        &format!("refresh subject {idx}"),
                        Some("deploy-fix"),
                    )
                },
            )
            .await?;
        }

        engine.refresh_constraint_cache().await?;

        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let stores = engine.episodic_store.read().await;
        let store = stores.get(&scope_id).expect("store exists");

        assert_eq!(store.cached_constraints.len(), 25);

        Ok(())
    }
}
