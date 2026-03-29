//! Episode CRUD operations and WORM ledger integration.

use super::privacy::{
    compute_expires_at, is_episode_suppressed, is_episode_suppressed_for_episode, scrub_episode,
};
use super::{CausalStep, Episode, EpisodeOutcome, EpisodeType, LinkType};
use crate::agent::engine::AgentEngine;
use crate::agent::types::{AgentEvent, GoalRun};

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};

fn episode_type_to_str(t: &EpisodeType) -> &'static str {
    match t {
        EpisodeType::GoalStart => "goal_start",
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

fn row_to_episode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Episode> {
    let episode_type_str: String = row.get(4)?;
    let outcome_str: String = row.get(8)?;
    let entities_json: String = row.get(10)?;
    let causal_chain_json: String = row.get(11)?;

    Ok(Episode {
        id: row.get(0)?,
        goal_run_id: row.get(1)?,
        thread_id: row.get(2)?,
        session_id: row.get(3)?,
        goal_text: row.get(5)?,
        goal_type: row.get(6)?,
        episode_type: str_to_episode_type(&episode_type_str),
        summary: row.get(7)?,
        outcome: str_to_episode_outcome(&outcome_str),
        root_cause: row.get(9)?,
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        causal_chain: serde_json::from_str(&causal_chain_json).unwrap_or_default(),
        solution_class: row.get(12)?,
        duration_ms: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
        tokens_used: row.get::<_, Option<i32>>(14)?.map(|v| v as u32),
        confidence: row.get(15)?,
        confidence_before: row.get(16)?,
        confidence_after: row.get(17)?,
        created_at: row.get::<_, i64>(18)? as u64,
        expires_at: row.get::<_, Option<i64>>(19)?.map(|v| v as u64),
    })
}

fn confidence_band_from_goal_run(goal_run: &GoalRun) -> Option<f64> {
    let title = goal_run.steps.first()?.title.as_str();
    if title.starts_with("[HIGH]") {
        Some(0.85)
    } else if title.starts_with("[MEDIUM]") {
        Some(0.65)
    } else if title.starts_with("[LOW]") {
        Some(0.35)
    } else {
        None
    }
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
        if is_episode_suppressed(&config) || is_episode_suppressed_for_episode(&config, &episode) {
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
                        id, goal_run_id, thread_id, session_id, goal_text, goal_type,
                        episode_type, summary, outcome, root_cause, entities, causal_chain,
                        solution_class, duration_ms, tokens_used, confidence,
                        confidence_before, confidence_after, created_at, expires_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                    params![
                        ep.id,
                        ep.goal_run_id,
                        ep.thread_id,
                        ep.session_id,
                        ep.goal_text,
                        ep.goal_type,
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
                        ep.confidence_before,
                        ep.confidence_after,
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

    pub(crate) async fn record_goal_start_episode(&self, goal_run: &GoalRun) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let episode = Episode {
            id: format!("ep_{}", uuid::Uuid::new_v4()),
            goal_run_id: Some(goal_run.id.clone()),
            thread_id: goal_run.thread_id.clone(),
            session_id: goal_run.session_id.clone(),
            goal_text: Some(goal_run.goal.clone()),
            goal_type: Some("goal_run".to_string()),
            episode_type: EpisodeType::GoalStart,
            summary: format!("{}: {}", goal_run.title, goal_run.goal),
            outcome: EpisodeOutcome::Partial,
            root_cause: None,
            entities: Vec::new(),
            causal_chain: Vec::new(),
            solution_class: None,
            duration_ms: None,
            tokens_used: None,
            confidence: None,
            confidence_before: None,
            confidence_after: None,
            created_at: now_ms,
            expires_at: None,
        };

        self.record_episode(episode.clone()).await?;
        self.link_related_goal_episode(&episode).await?;
        Ok(())
    }

    /// Record an episode derived from a completed or failed goal run.
    ///
    /// Extracts entities from goal steps (file paths from instructions, step titles)
    /// and builds a causal chain from failed steps.
    pub(crate) async fn record_goal_episode(
        &self,
        goal_run: &GoalRun,
        outcome: EpisodeOutcome,
    ) -> Result<()> {
        let episode_type = match outcome {
            EpisodeOutcome::Success => EpisodeType::GoalCompletion,
            _ => EpisodeType::GoalFailure,
        };

        // Build summary: truncated to 500 chars
        let raw_summary = format!("{}: {}", goal_run.title, goal_run.goal);
        let summary = if raw_summary.len() > 500 {
            format!("{}...", &raw_summary[..497])
        } else {
            raw_summary
        };

        // Root cause: for failures, use last_error or failure_cause
        let root_cause = if outcome == EpisodeOutcome::Failure {
            goal_run
                .last_error
                .clone()
                .or_else(|| goal_run.failure_cause.clone())
        } else {
            None
        };

        // Build entities from steps
        let mut entities = Vec::new();
        let file_re =
            regex::Regex::new(r"(?:^|\s)((?:[\w.\-]+/)*[\w.\-]+\.\w+)").unwrap_or_else(|_| {
                // Fallback: this regex should always compile
                regex::Regex::new(r"\S+\.\w+").unwrap()
            });
        for step in &goal_run.steps {
            entities.push(format!("step:{}", step.title));
            for cap in file_re.captures_iter(&step.instructions) {
                if let Some(m) = cap.get(1) {
                    entities.push(format!("file:{}", m.as_str()));
                }
            }
        }
        entities.sort();
        entities.dedup();

        // Build causal chain from failed steps
        let causal_chain: Vec<CausalStep> = goal_run
            .steps
            .iter()
            .filter(|s| {
                s.status == crate::agent::types::GoalRunStepStatus::Failed && s.error.is_some()
            })
            .map(|s| CausalStep {
                step: s.title.clone(),
                cause: s.error.clone().unwrap_or_else(|| "unknown".to_string()),
                effect: "step failed".to_string(),
            })
            .collect();

        // Duration
        let duration_ms =
            goal_run
                .duration_ms
                .or_else(|| match (goal_run.started_at, goal_run.completed_at) {
                    (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                    _ => None,
                });

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let episode = Episode {
            id: format!("ep_{}", uuid::Uuid::new_v4()),
            goal_run_id: Some(goal_run.id.clone()),
            thread_id: goal_run.thread_id.clone(),
            session_id: goal_run.session_id.clone(),
            goal_text: Some(goal_run.goal.clone()),
            goal_type: Some("goal_run".to_string()),
            episode_type,
            summary,
            outcome,
            root_cause,
            entities,
            causal_chain,
            solution_class: None,
            duration_ms,
            tokens_used: None,
            confidence: None,
            confidence_before: confidence_band_from_goal_run(goal_run),
            confidence_after: Some(match outcome {
                EpisodeOutcome::Success => 1.0,
                EpisodeOutcome::Failure => 0.0,
                EpisodeOutcome::Partial => 0.5,
                EpisodeOutcome::Abandoned => 0.25,
            }),
            created_at: now_ms,
            expires_at: None,
        };

        let episode_ref = episode.clone();
        self.record_episode(episode).await?;

        // Auto-create negative knowledge constraint from failed episodes (NKNO-01, NKNO-02)
        if outcome == EpisodeOutcome::Failure {
            if let Err(e) = self
                .record_negative_knowledge_from_episode(&episode_ref)
                .await
            {
                tracing::warn!("Failed to record negative knowledge from episode: {e}");
            }
        }

        Ok(())
    }

    /// Record an episode when a session ends (EPIS-08).
    pub(crate) async fn record_session_end_episode(
        &self,
        thread_id: &str,
        session_summary: &str,
        entities: Vec<String>,
    ) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let episode = Episode {
            id: format!("ep_{}", uuid::Uuid::new_v4()),
            goal_run_id: None,
            thread_id: Some(thread_id.to_string()),
            session_id: Some(thread_id.to_string()),
            goal_text: None,
            goal_type: None,
            episode_type: EpisodeType::SessionEnd,
            summary: session_summary.to_string(),
            outcome: EpisodeOutcome::Success,
            root_cause: None,
            entities,
            causal_chain: Vec::new(),
            solution_class: None,
            duration_ms: None,
            tokens_used: None,
            confidence: None,
            confidence_before: None,
            confidence_after: None,
            created_at: now_ms,
            expires_at: None,
        };

        self.record_episode(episode).await
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
                                goal_text, goal_type, summary, outcome, root_cause, entities, causal_chain,
                                solution_class, duration_ms, tokens_used, confidence, confidence_before, confidence_after,
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
                            goal_text, goal_type, summary, outcome, root_cause, entities, causal_chain,
                            solution_class, duration_ms, tokens_used, confidence, confidence_before, confidence_after,
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

    /// Expire (delete) old episodes past their TTL (EPIS-09).
    /// Rebuilds FTS5 index after deletion to remove stale entries.
    pub(crate) async fn expire_old_episodes(&self) -> Result<usize> {
        let now_ms = super::super::now_millis() as i64;
        self.history
            .conn
            .call(move |conn| {
                let deleted = conn.execute(
                    "DELETE FROM episodes WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                    rusqlite::params![now_ms],
                )?;
                if deleted > 0 {
                    // Rebuild FTS5 index to remove stale entries
                    conn.execute(
                        "INSERT INTO episodes_fts(episodes_fts) VALUES('rebuild')",
                        [],
                    )
                    .ok();
                }
                Ok(deleted)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn link_related_goal_episode(&self, episode: &Episode) -> Result<()> {
        let Some(goal_text) = episode.goal_text.clone() else {
            return Ok(());
        };
        let current_episode_id = episode.id.clone();
        let related: Option<(String, String)> = self
            .history
            .conn
            .call(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT id, outcome FROM episodes
                     WHERE goal_text = ?1 AND id != ?2
                     ORDER BY created_at DESC
                     LIMIT 1",
                        params![goal_text, current_episode_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let Some((target_episode_id, outcome)) = related else {
            return Ok(());
        };
        let link_type = match outcome.as_str() {
            "failure" | "abandoned" => LinkType::RetryOf,
            "success" | "partial" => LinkType::BuildsOn,
            _ => LinkType::BuildsOn,
        };
        self.create_episode_link(super::EpisodeLink {
            id: format!("el_{}", uuid::Uuid::new_v4()),
            source_episode_id: episode.id.clone(),
            target_episode_id,
            link_type,
            evidence: Some("Auto-linked from matching goal_text".to_string()),
            created_at: episode.created_at,
        })
        .await
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
