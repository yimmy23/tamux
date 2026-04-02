//! Episode link creation and querying.

use super::{EpisodeLink, LinkType};
use crate::agent::engine::AgentEngine;

use anyhow::Result;
use rusqlite::params;

fn link_type_to_str(t: &LinkType) -> &'static str {
    match t {
        LinkType::RetryOf => "retry_of",
        LinkType::BuildsOn => "builds_on",
        LinkType::Contradicts => "contradicts",
        LinkType::Supersedes => "supersedes",
        LinkType::CausedBy => "caused_by",
    }
}

fn str_to_link_type(s: &str) -> LinkType {
    match s {
        "retry_of" => LinkType::RetryOf,
        "builds_on" => LinkType::BuildsOn,
        "contradicts" => LinkType::Contradicts,
        "supersedes" => LinkType::Supersedes,
        _ => LinkType::CausedBy,
    }
}

fn row_to_episode_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<EpisodeLink> {
    let link_type_str: String = row.get(4)?;
    Ok(EpisodeLink {
        id: row.get(0)?,
        source_episode_id: row.get(2)?,
        target_episode_id: row.get(3)?,
        link_type: str_to_link_type(&link_type_str),
        evidence: row.get(5)?,
        created_at: row.get::<_, i64>(6)? as u64,
    })
}

impl AgentEngine {
    /// Create a directed link between two episodes.
    pub(crate) async fn create_episode_link(&self, link: EpisodeLink) -> Result<()> {
        let link_type_str = link_type_to_str(&link.link_type).to_string();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO episode_links (
                        id, agent_id, source_episode_id, target_episode_id, link_type, evidence, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        link.id,
                        agent_id,
                        link.source_episode_id,
                        link.target_episode_id,
                        link_type_str,
                        link.evidence,
                        link.created_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Get all links involving a specific episode (as source or target).
    pub(crate) async fn get_episode_links(&self, episode_id: &str) -> Result<Vec<EpisodeLink>> {
        let episode_id = episode_id.to_string();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, source_episode_id, target_episode_id, link_type, evidence, created_at
                     FROM episode_links
                     WHERE (source_episode_id = ?1 OR target_episode_id = ?1)
                       AND (agent_id = ?2 OR (?3 = 1 AND agent_id IS NULL))
                     ORDER BY created_at DESC",
                )?;
                let rows =
                    stmt.query_map(params![episode_id, agent_id, include_legacy], row_to_episode_link)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Find episode IDs linked to a given episode by a specific link type.
    ///
    /// Searches both directions (as source and as target).
    pub(crate) async fn find_linked_episodes(
        &self,
        episode_id: &str,
        link_type: LinkType,
    ) -> Result<Vec<String>> {
        let episode_id = episode_id.to_string();
        let link_type_str = link_type_to_str(&link_type).to_string();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT target_episode_id FROM episode_links
                     WHERE source_episode_id = ?1 AND link_type = ?2 AND (agent_id = ?3 OR (?4 = 1 AND agent_id IS NULL))
                     UNION
                     SELECT source_episode_id FROM episode_links
                     WHERE target_episode_id = ?1 AND link_type = ?2 AND (agent_id = ?3 OR (?4 = 1 AND agent_id IS NULL))",
                )?;
                let rows = stmt.query_map(params![episode_id, link_type_str, agent_id, include_legacy], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
