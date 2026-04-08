mod metadata;
mod ranking;
mod types;

use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::HistoryStore;
use anyhow::{Context, Result};
use ranking::rank_skill_candidates;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use types::SkillCandidateInput;

pub(crate) use metadata::extract_skill_metadata;
pub(crate) use types::{
    SkillDiscoveryResult, SkillRecommendationAction, SkillRecommendationConfidence,
};

const MAX_SKILL_EXCERPT_LINES: usize = 40;
const MAX_SKILL_EXCERPT_CHARS: usize = 2400;

pub(crate) async fn discover_local_skills(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    sync_skill_catalog(history, skills_root).await?;

    let mut candidates = Vec::new();
    for record in history.list_skill_variants(None, 512).await? {
        if matches!(record.status.as_str(), "archived" | "merged" | "draft") {
            continue;
        }

        let skill_path = skills_root.join(&record.relative_path);
        let content = std::fs::read_to_string(&skill_path).with_context(|| {
            format!(
                "failed to read skill recommendation file {}",
                skill_path.display()
            )
        })?;
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&record.relative_path, &content),
            excerpt: excerpt_skill(&content),
            record,
        });
    }

    Ok(rank_skill_candidates(
        candidates,
        query,
        workspace_tags,
        limit,
        cfg,
    ))
}

pub(crate) async fn sync_skill_catalog(history: &HistoryStore, skills_root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    for path in files {
        history.register_skill_document(&path).await?;
    }
    Ok(())
}

pub(crate) async fn discover_community_skills(
    data_dir: &Path,
    registry_url: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<amux_protocol::CommunitySkillEntry>> {
    let client = RegistryClient::new(registry_url.to_string(), data_dir);
    let _ = client.refresh_index().await;
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    let mut queries = vec![query.trim().to_string()];
    queries.extend(
        query
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
            .map(str::trim)
            .filter(|token| token.len() >= 3)
            .map(ToOwned::to_owned),
    );

    for search_query in queries {
        if search_query.is_empty() {
            continue;
        }
        for entry in client.search(&search_query).await? {
            if seen.insert(entry.name.clone()) {
                merged.push(entry);
            }
            if merged.len() >= limit.max(1) {
                return Ok(merged
                    .into_iter()
                    .map(|entry| to_community_entry(&entry))
                    .collect());
            }
        }
    }

    Ok(merged
        .into_iter()
        .map(|entry| to_community_entry(&entry))
        .collect())
}

fn collect_skill_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_documents(&path, out)?;
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.eq_ignore_ascii_case("skill.md"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn excerpt_skill(content: &str) -> String {
    let mut excerpt = content
        .lines()
        .take(MAX_SKILL_EXCERPT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if excerpt.len() > MAX_SKILL_EXCERPT_CHARS {
        let new_len = excerpt
            .char_indices()
            .take_while(|(idx, ch)| *idx + ch.len_utf8() <= MAX_SKILL_EXCERPT_CHARS)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0);
        excerpt.truncate(new_len);
        excerpt.push_str("\n...");
    } else if content.lines().count() > MAX_SKILL_EXCERPT_LINES {
        excerpt.push_str("\n...");
    }
    excerpt
}

#[cfg(test)]
#[path = "../tests/skill_recommendation.rs"]
mod tests;
