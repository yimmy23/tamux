//! Episode CRUD operations and WORM ledger integration.

use super::privacy::{compute_expires_at, is_episode_suppressed, scrub_episode};
use super::{Episode, EpisodeOutcome, EpisodeType};
use crate::agent::engine::AgentEngine;
use crate::agent::types::AgentEvent;

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};

fn episode_type_to_str(t: &EpisodeType) -> &'static str {
    match t {
        EpisodeType::GoalCompletion => "goal_completion",
        EpisodeType::GoalFailure => "goal_failure",
        EpisodeType::SessionEnd => "session_end",
        EpisodeType::Discovery => "discovery",
    }
}

fn episode_outcome_to_str(o: &EpisodeOutcome) -> &'static str {
    match o {
        EpisodeOutcome::Success => "success",
        EpisodeOutcome::Failure => "failure",
        EpisodeOutcome::Partial => "partial",
        EpisodeOutcome::Abandoned => "abandoned",
    }
}

fn str_to_episode_type(s: &str) -> EpisodeType {
    match s {
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

fn row_to_episode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Episode> {
    let episode_type_str: String = row.get(4)?;
    let outcome_str: String = row.get(6)?;
    let entities_json: String = row.get(8)?;
    let causal_chain_json: String = row.get(9)?;

    Ok(Episode {
        id: row.get(0)?,
        goal_run_id: row.get(1)?,
        thread_id: row.get(2)?,
        session_id: row.get(3)?,
        episode_type: str_to_episode_type(&episode_type_str),
        summary: row.get(5)?,
        outcome: str_to_episode_outcome(&outcome_str),
        root_cause: row.get(7)?,
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        causal_chain: serde_json::from_str(&causal_chain_json).unwrap_or_default(),
        solution_class: row.get(10)?,
        duration_ms: row.get::<_, Option<i64>>(11)?.map(|v| v as u64),
        tokens_used: row.get::<_, Option<i32>>(12)?.map(|v| v as u32),
        confidence: row.get(13)?,
        created_at: row.get::<_, i64>(14)? as u64,
        expires_at: row.get::<_, Option<i64>>(15)?.map(|v| v as u64),
    })
}

impl AgentEngine {
    /// Record an episode to the episodic memory store.
    ///
    /// This method:
    /// 1. Checks suppression config (returns early if suppressed)
    /// 2. Scrubs PII from text fields
    /// 3. Computes TTL-based expiration if not already set
    /// 4. Inserts into SQLite
    /// 5. Emits an `EpisodeRecorded` event
    /// 6. Appends to the WORM episodic ledger
    pub(crate) async fn record_episode(&self, mut episode: Episode) -> Result<()> {
        let config = self.config.read().await.episodic.clone();

        // Check suppression
        if is_episode_suppressed(&config) {
            return Ok(());
        }

        // Scrub PII
        scrub_episode(&mut episode);

        // Compute expires_at if not already set
        if episode.expires_at.is_none() {
            episode.expires_at = compute_expires_at(episode.created_at, config.episode_ttl_days);
        }

        let ep = episode.clone();
        let episode_type_str = episode_type_to_str(&ep.episode_type).to_string();
        let outcome_str = episode_outcome_to_str(&ep.outcome).to_string();
        let entities_json = ep.entities_json();
        let causal_chain_json = ep.causal_chain_json();

        // Insert into SQLite
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO episodes (
                        id, goal_run_id, thread_id, session_id, episode_type,
                        summary, outcome, root_cause, entities, causal_chain,
                        solution_class, duration_ms, tokens_used, confidence,
                        created_at, expires_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        ep.id,
                        ep.goal_run_id,
                        ep.thread_id,
                        ep.session_id,
                        episode_type_str,
                        ep.summary,
                        outcome_str,
                        ep.root_cause,
                        entities_json,
                        causal_chain_json,
                        ep.solution_class,
                        ep.duration_ms.map(|v| v as i64),
                        ep.tokens_used.map(|v| v as i32),
                        ep.confidence,
                        ep.created_at as i64,
                        ep.expires_at.map(|v| v as i64),
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Emit event
        let _ = self.event_tx.send(AgentEvent::EpisodeRecorded {
            episode_id: episode.id.clone(),
            episode_type: episode_type_to_str(&episode.episode_type).to_string(),
            outcome: episode_outcome_to_str(&episode.outcome).to_string(),
            summary: episode.summary.clone(),
        });

        // WORM ledger append
        if let Err(e) = self.append_episodic_worm(&episode).await {
            tracing::warn!(episode_id = %episode.id, error = %e, "failed to append episodic WORM entry");
        }

        Ok(())
    }

    /// Retrieve a single episode by ID.
    pub(crate) async fn get_episode(&self, episode_id: &str) -> Result<Option<Episode>> {
        let episode_id = episode_id.to_string();
        self.history
            .conn
            .call(move |conn| {
                let result = conn
                    .query_row(
                        "SELECT id, goal_run_id, thread_id, session_id, episode_type,
                                summary, outcome, root_cause, entities, causal_chain,
                                solution_class, duration_ms, tokens_used, confidence,
                                created_at, expires_at
                         FROM episodes WHERE id = ?1",
                        params![episode_id],
                        row_to_episode,
                    )
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List all episodes for a given goal run, ordered by creation time descending.
    pub(crate) async fn list_episodes_for_goal_run(
        &self,
        goal_run_id: &str,
    ) -> Result<Vec<Episode>> {
        let goal_run_id = goal_run_id.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, goal_run_id, thread_id, session_id, episode_type,
                            summary, outcome, root_cause, entities, causal_chain,
                            solution_class, duration_ms, tokens_used, confidence,
                            created_at, expires_at
                     FROM episodes WHERE goal_run_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![goal_run_id], row_to_episode)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Append an episode record to the WORM episodic ledger file.
    async fn append_episodic_worm(&self, episode: &Episode) -> Result<()> {
        let worm_path = self.data_dir.join("worm/episodic-ledger.jsonl");

        // Ensure worm directory exists
        if let Some(parent) = worm_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let payload = serde_json::to_value(episode)?;
        let payload_json = serde_json::to_string(&payload)?;

        // Read last entry for chain continuity
        let (prev_hash, seq) = {
            let path = worm_path.clone();
            tokio::task::spawn_blocking(move || read_last_worm_entry(&path))
                .await
                .unwrap_or_else(|_| ("genesis".to_string(), 0))
        };

        let hash = hex_hash(&format!("{}{}", prev_hash, payload_json));
        let worm_line = serde_json::to_string(&json!({
            "seq": seq,
            "prev_hash": prev_hash,
            "hash": hash,
            "timestamp": episode.created_at,
            "payload": payload,
        }))?;

        // Append to ledger file
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&worm_path)
            .await?;
        file.write_all(worm_line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }
}

/// Compute SHA-256 hex hash of input string.
fn hex_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Read the last line of a WORM ledger file and extract (prev_hash, next_seq).
/// Returns ("genesis", 0) if the file does not exist or is empty.
fn read_last_worm_entry(path: &std::path::Path) -> (String, usize) {
    use std::io::BufRead;

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return ("genesis".to_string(), 0),
    };

    let reader = std::io::BufReader::new(file);
    let mut last_line: Option<String> = None;
    for line in reader.lines() {
        if let Ok(l) = line {
            if !l.trim().is_empty() {
                last_line = Some(l);
            }
        }
    }

    match last_line {
        None => ("genesis".to_string(), 0),
        Some(line) => {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
                let hash = entry
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("genesis")
                    .to_string();
                let seq = entry
                    .get("seq")
                    .and_then(|v| v.as_u64())
                    .map(|s| s as usize + 1)
                    .unwrap_or(0);
                (hash, seq)
            } else {
                ("genesis".to_string(), 0)
            }
        }
    }
}
