use super::super::EpisodeLink;
use super::*;

use serde_json::json;
use sha2::{Digest, Sha256};

impl AgentEngine {
    /// Append an episode record to the WORM episodic ledger file.
    pub(super) async fn append_episodic_worm(&self, episode: &Episode) -> Result<()> {
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

    pub(super) async fn link_related_goal_episode(&self, episode: &Episode) -> Result<()> {
        let Some(goal_text) = episode.goal_text.clone() else {
            return Ok(());
        };
        let current_episode_id = episode.id.clone();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        let related: Option<(String, String)> = self
            .history
            .conn
            .call(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT id, outcome FROM episodes
                     WHERE goal_text = ?1
                       AND id != ?2
                       AND (agent_id = ?3 OR (?4 = 1 AND agent_id IS NULL))
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC
                     LIMIT 1",
                        params![goal_text, current_episode_id, agent_id, include_legacy],
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
        self.create_episode_link(EpisodeLink {
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
        if let Ok(line) = line {
            if !line.trim().is_empty() {
                last_line = Some(line);
            }
        }
    }

    match last_line {
        None => ("genesis".to_string(), 0),
        Some(line) => {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
                let hash = entry
                    .get("hash")
                    .and_then(|value| value.as_str())
                    .unwrap_or("genesis")
                    .to_string();
                let seq = entry
                    .get("seq")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize + 1)
                    .unwrap_or(0);
                (hash, seq)
            } else {
                ("genesis".to_string(), 0)
            }
        }
    }
}
