#![allow(dead_code)]

//! FTS5 retrieval engine with multi-signal ranking for episodic memory.
//!
//! Provides BM25 + recency-weighted retrieval, temporal and entity-scoped queries,
//! and episodic context formatting for injection into system prompts and goal planning.

use super::{Episode, EpisodeOutcome, EpisodeType};
use crate::agent::engine::AgentEngine;

use anyhow::Result;
use rusqlite::params;

// ---------------------------------------------------------------------------
// FTS5 query helpers
// ---------------------------------------------------------------------------

/// Escape and format a raw user query into an FTS5 OR query.
///
/// Strips FTS5 special characters, splits on whitespace, joins with OR.
/// Returns `"*"` (match all) if the result would be empty.
pub fn format_fts5_query(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();

    let terms: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .collect();

    if terms.is_empty() {
        return "*".to_string();
    }

    terms.join(" OR ")
}

/// Compute a recency weight using exponential decay.
///
/// Half-life is approximately 14 days (`-0.05 * age_days`).
/// Result clamped to `[0.01, 1.0]`.
pub fn compute_recency_weight(episode_created_at: u64, now_ms: u64) -> f64 {
    let age_days = (now_ms.saturating_sub(episode_created_at)) as f64 / (86400.0 * 1000.0);
    let weight = (-0.05 * age_days).exp();
    weight.clamp(0.01, 1.0)
}

// ---------------------------------------------------------------------------
// Episodic context formatting
// ---------------------------------------------------------------------------

/// Format a slice of episodes into a human-readable episodic context section
/// suitable for injection into a system prompt or goal planning context.
///
/// Respects `max_tokens` budget (approximate: 4 chars per token).
/// Labels: WARNING (Failure), CAUTION (Partial), REFERENCE (Success), NOTE (Abandoned).
pub fn format_episodic_context(episodes: &[Episode], max_tokens: usize) -> String {
    if episodes.is_empty() {
        return String::new();
    }

    let max_chars = max_tokens * 4;
    let mut output = String::from("## Past Experience (Episodic Memory)\n");
    let total = episodes.len();

    for (i, episode) in episodes.iter().enumerate() {
        let label = match episode.outcome {
            EpisodeOutcome::Failure => "WARNING",
            EpisodeOutcome::Partial => "CAUTION",
            EpisodeOutcome::Success => "REFERENCE",
            EpisodeOutcome::Abandoned => "NOTE",
        };

        let age = format_age(episode.created_at);

        let mut entry = format!("{}: \"{}\" ({} ago)\n", label, episode.summary, age);

        if let Some(ref root_cause) = episode.root_cause {
            entry.push_str(&format!("  Root cause: {}\n", root_cause));
        }

        if !episode.entities.is_empty() {
            entry.push_str(&format!("  Entities: {}\n", episode.entities.join(", ")));
        }

        entry.push_str(&format!("  Link: episode_{}\n\n", episode.id));

        if output.len() + entry.len() > max_chars {
            let remaining = total - i;
            output.push_str(&format!(
                "\n({} more episodes omitted due to token budget)\n",
                remaining
            ));
            break;
        }

        output.push_str(&entry);
    }

    output
}

/// Format an age in milliseconds as a human-readable duration string.
fn format_age(created_at: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let age_ms = now_ms.saturating_sub(created_at);
    let age_days = age_ms / (86400 * 1000);
    let age_hours = (age_ms % (86400 * 1000)) / (3600 * 1000);

    if age_days > 0 {
        format!("{}d {}h", age_days, age_hours)
    } else if age_hours > 0 {
        format!("{}h", age_hours)
    } else {
        "just now".to_string()
    }
}

// ---------------------------------------------------------------------------
// Row mapping helper (reusable across retrieval queries)
// ---------------------------------------------------------------------------

fn row_to_episode_with_rank(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Episode, f64)> {
    let episode_type_str: String = row.get(5)?;
    let outcome_str: String = row.get(9)?;
    let entities_json: String = row.get(11)?;
    let causal_chain_json: String = row.get(12)?;
    let rank: f64 = row.get(21)?;

    let episode = Episode {
        id: row.get(0)?,
        goal_run_id: row.get(2)?,
        thread_id: row.get(3)?,
        session_id: row.get(4)?,
        goal_text: row.get(6)?,
        goal_type: row.get(7)?,
        episode_type: str_to_episode_type(&episode_type_str),
        summary: row.get(8)?,
        outcome: str_to_episode_outcome(&outcome_str),
        root_cause: row.get(10)?,
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        causal_chain: serde_json::from_str(&causal_chain_json).unwrap_or_default(),
        solution_class: row.get(13)?,
        duration_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
        tokens_used: row.get::<_, Option<i32>>(15)?.map(|v| v as u32),
        confidence: row.get(16)?,
        confidence_before: row.get(17)?,
        confidence_after: row.get(18)?,
        created_at: row.get::<_, i64>(19)? as u64,
        expires_at: row.get::<_, Option<i64>>(20)?.map(|v| v as u64),
    };

    Ok((episode, rank))
}

fn row_to_episode_plain(row: &rusqlite::Row<'_>) -> rusqlite::Result<Episode> {
    let episode_type_str: String = row.get(7)?;
    let outcome_str: String = row.get(9)?;
    let entities_json: String = row.get(11)?;
    let causal_chain_json: String = row.get(12)?;

    Ok(Episode {
        id: row.get(0)?,
        goal_run_id: row.get(2)?,
        thread_id: row.get(3)?,
        session_id: row.get(4)?,
        goal_text: row.get(5)?,
        goal_type: row.get(6)?,
        episode_type: str_to_episode_type(&episode_type_str),
        summary: row.get(8)?,
        outcome: str_to_episode_outcome(&outcome_str),
        root_cause: row.get(10)?,
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        causal_chain: serde_json::from_str(&causal_chain_json).unwrap_or_default(),
        solution_class: row.get(13)?,
        duration_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
        tokens_used: row.get::<_, Option<i32>>(15)?.map(|v| v as u32),
        confidence: row.get(16)?,
        confidence_before: row.get(17)?,
        confidence_after: row.get(18)?,
        created_at: row.get::<_, i64>(19)? as u64,
        expires_at: row.get::<_, Option<i64>>(20)?.map(|v| v as u64),
    })
}

fn str_to_episode_type(s: &str) -> EpisodeType {
    match s {
        "goal_start" => EpisodeType::GoalStart,
        "goal_completion" => EpisodeType::GoalCompletion,
        "goal_failure" => EpisodeType::GoalFailure,
        "session_end" => EpisodeType::SessionEnd,
        _ => EpisodeType::Discovery,
    }
}

fn str_to_episode_outcome(s: &str) -> EpisodeOutcome {
    match s {
        "success" => EpisodeOutcome::Success,
        "failure" => EpisodeOutcome::Failure,
        "partial" => EpisodeOutcome::Partial,
        _ => EpisodeOutcome::Abandoned,
    }
}

// ---------------------------------------------------------------------------
// Retrieval methods on AgentEngine
// ---------------------------------------------------------------------------

impl AgentEngine {
    /// Retrieve relevant episodes using FTS5 BM25 ranking with recency re-weighting.
    ///
    /// Steps:
    /// 1. Check if episodic memory is enabled
    /// 2. Over-fetch from FTS5 (3x limit for re-ranking headroom)
    /// 3. Re-rank by combined score: `bm25_rank * recency_weight`
    /// 4. Return top `limit` results (capped by `max_retrieval_episodes` config)
    pub(crate) async fn retrieve_relevant_episodes(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Episode>> {
        let config = self.config.read().await;
        let ep_config = config.episodic.clone();
        drop(config);

        if !ep_config.enabled {
            return Ok(Vec::new());
        }

        let effective_limit = limit.min(ep_config.max_retrieval_episodes);
        let fts5_query = format_fts5_query(query);
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let over_fetch = effective_limit * 3;

        let rows = self
            .history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT e.id, e.agent_id, e.goal_run_id, e.thread_id, e.session_id, e.episode_type,
                            e.goal_text, e.goal_type, e.summary, e.outcome, e.root_cause, e.entities, e.causal_chain,
                            e.solution_class, e.duration_ms, e.tokens_used, e.confidence, e.confidence_before, e.confidence_after,
                            e.created_at, e.expires_at,
                            bm25(episodes_fts) as rank
                     FROM episodes e
                     JOIN episodes_fts ON e.rowid = episodes_fts.rowid
                     WHERE episodes_fts MATCH ?1
                       AND (e.agent_id = ?2 OR (?3 = 1 AND e.agent_id IS NULL))
                       AND (e.expires_at IS NULL OR e.expires_at > ?4)
                       AND e.deleted_at IS NULL
                     ORDER BY rank
                     LIMIT ?5",
                )?;
                let rows = stmt.query_map(
                    params![fts5_query, agent_id, include_legacy, now_ms as i64, over_fetch as i64],
                    row_to_episode_with_rank,
                )?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Re-rank by combined score: bm25 rank (negative, lower is better) * recency weight
        let mut scored: Vec<(Episode, f64)> = rows
            .into_iter()
            .map(|(ep, bm25_rank)| {
                let recency = compute_recency_weight(ep.created_at, now_ms);
                // BM25 scores are negative (lower = more relevant), so we negate for sorting
                let combined = (-bm25_rank) * recency;
                (ep, combined)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(effective_limit);

        Ok(scored.into_iter().map(|(ep, _)| ep).collect())
    }

    /// Retrieve episodes matching a query within a time range (EPIS-04).
    pub(crate) async fn search_episodes_temporal(
        &self,
        query: &str,
        since_ms: u64,
        limit: usize,
    ) -> Result<Vec<Episode>> {
        let config = self.config.read().await;
        let ep_config = config.episodic.clone();
        drop(config);

        if !ep_config.enabled {
            return Ok(Vec::new());
        }

        let effective_limit = limit.min(ep_config.max_retrieval_episodes);
        let fts5_query = format_fts5_query(query);
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let over_fetch = effective_limit * 3;

        let rows = self
            .history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT e.id, e.agent_id, e.goal_run_id, e.thread_id, e.session_id, e.episode_type,
                            e.goal_text, e.goal_type, e.summary, e.outcome, e.root_cause, e.entities, e.causal_chain,
                            e.solution_class, e.duration_ms, e.tokens_used, e.confidence, e.confidence_before, e.confidence_after,
                            e.created_at, e.expires_at,
                            bm25(episodes_fts) as rank
                     FROM episodes e
                     JOIN episodes_fts ON e.rowid = episodes_fts.rowid
                     WHERE episodes_fts MATCH ?1
                       AND (e.agent_id = ?2 OR (?3 = 1 AND e.agent_id IS NULL))
                       AND (e.expires_at IS NULL OR e.expires_at > ?4)
                       AND e.created_at >= ?5
                       AND e.deleted_at IS NULL
                     ORDER BY rank
                     LIMIT ?6",
                )?;
                let rows = stmt.query_map(
                    params![
                        fts5_query,
                        agent_id,
                        include_legacy,
                        now_ms as i64,
                        since_ms as i64,
                        over_fetch as i64
                    ],
                    row_to_episode_with_rank,
                )?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut scored: Vec<(Episode, f64)> = rows
            .into_iter()
            .map(|(ep, bm25_rank)| {
                let recency = compute_recency_weight(ep.created_at, now_ms);
                let combined = (-bm25_rank) * recency;
                (ep, combined)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(effective_limit);

        Ok(scored.into_iter().map(|(ep, _)| ep).collect())
    }

    /// Retrieve episodes by entity name using LIKE match (EPIS-05).
    pub(crate) async fn search_episodes_by_entity(
        &self,
        entity: &str,
        limit: usize,
    ) -> Result<Vec<Episode>> {
        let config = self.config.read().await;
        let ep_config = config.episodic.clone();
        drop(config);

        if !ep_config.enabled {
            return Ok(Vec::new());
        }

        let effective_limit = limit.min(ep_config.max_retrieval_episodes);
        let entity_pattern = format!("%{}%", entity);
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, goal_run_id, thread_id, session_id, goal_text, goal_type, episode_type,
                            summary, outcome, root_cause, entities, causal_chain,
                            solution_class, duration_ms, tokens_used, confidence, confidence_before, confidence_after,
                            created_at, expires_at
                     FROM episodes
                     WHERE entities LIKE ?1
                       AND (agent_id = ?2 OR (?3 = 1 AND agent_id IS NULL))
                       AND (expires_at IS NULL OR expires_at > ?4)
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC
                     LIMIT ?5",
                )?;
                let rows = stmt.query_map(
                    params![
                        entity_pattern,
                        agent_id,
                        include_legacy,
                        now_ms as i64,
                        effective_limit as i64
                    ],
                    row_to_episode_plain,
                )?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

#[cfg(test)]
#[path = "retrieval/tests.rs"]
mod tests;
