mod metadata;
mod ranking;
mod types;

use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::{derive_skill_metadata, HistoryStore, SkillVariantRecord};
use anyhow::{Context, Result};
use base64::Engine;
use ranking::rank_skill_candidates;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use types::SkillCandidateInput;

pub(crate) use metadata::extract_skill_metadata;
pub(crate) use types::{
    SkillDiscoveryResult, SkillDocumentMetadata, SkillRecommendation, SkillRecommendationAction,
    SkillRecommendationConfidence,
};

const MAX_SKILL_EXCERPT_LINES: usize = 40;
const MAX_SKILL_EXCERPT_CHARS: usize = 2400;
const DISCOVERY_CURSOR_PREFIX: &str = "skill-discovery:";

pub(crate) async fn discover_local_skills(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    let records = history.list_skill_variants(None, 512).await?;
    let candidates = if records.is_empty() {
        schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
        collect_filesystem_skill_candidates(skills_root)?
    } else {
        let candidates = collect_registered_skill_candidates(skills_root, records)?;
        if candidates.is_empty() {
            schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
            collect_filesystem_skill_candidates(skills_root)?
        } else {
            candidates
        }
    };

    Ok(rank_skill_candidates(
        candidates,
        query,
        workspace_tags,
        limit,
        cfg,
    ))
}

fn schedule_background_skill_catalog_sync(history: HistoryStore, skills_root: PathBuf) {
    tokio::spawn(async move {
        if let Err(error) = sync_skill_catalog(&history, &skills_root).await {
            tracing::warn!(
                %error,
                skills_root = %skills_root.display(),
                "background skill catalog sync failed during discovery"
            );
        }
    });
}

pub(crate) async fn sync_skill_catalog(history: &HistoryStore, skills_root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    for path in files {
        history.register_skill_document(&path).await?;
    }
    Ok(())
}

pub(crate) fn resolve_skill_document_path(
    skills_root: &Path,
    relative_path: &str,
) -> (PathBuf, String) {
    let normalized = relative_path.replace('\\', "/");
    let candidate = skills_root.join(&normalized);
    if candidate.exists() {
        return (candidate, normalized);
    }

    if let Some(stripped) = normalized.strip_prefix("builtin/") {
        let migrated = skills_root.join(stripped);
        if migrated.exists() {
            return (migrated, stripped.to_string());
        }
    }

    if let Some(resolved) = resolve_skill_document_by_suffix(skills_root, &normalized) {
        return resolved;
    }

    (candidate, normalized)
}

fn resolve_skill_document_by_suffix(
    skills_root: &Path,
    relative_path: &str,
) -> Option<(PathBuf, String)> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files).ok()?;

    for suffix in relative_path_suffixes(relative_path) {
        let matches = files
            .iter()
            .filter_map(|path| {
                let relative = path
                    .strip_prefix(skills_root)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == suffix || relative.ends_with(&format!("/{suffix}")) {
                    Some((path.clone(), relative))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return matches.into_iter().next();
        }
    }

    None
}

fn relative_path_suffixes(relative_path: &str) -> Vec<String> {
    let mut suffixes = Vec::new();
    let mut current = relative_path.trim_matches('/').to_string();
    while !current.is_empty() {
        suffixes.push(current.clone());
        let Some((_, tail)) = current.split_once('/') else {
            break;
        };
        current = tail.to_string();
    }
    suffixes
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
        } else if should_include_skill_document(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn should_include_skill_document(path: &Path) -> bool {
    should_include_skill_relative_path(&path.to_string_lossy())
}

fn should_include_skill_relative_path(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    file_name.eq_ignore_ascii_case("skill.md")
        || (path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"))
            && path
                .components()
                .any(|component| component.as_os_str() == "generated"))
}

fn collect_registered_skill_candidates(
    skills_root: &Path,
    records: Vec<SkillVariantRecord>,
) -> Result<Vec<SkillCandidateInput>> {
    let mut candidates = Vec::new();
    for record in records {
        if matches!(record.status.as_str(), "archived" | "merged" | "draft") {
            continue;
        }
        if !should_include_skill_relative_path(&record.relative_path) {
            continue;
        }

        let (skill_path, metadata_relative_path) =
            resolve_skill_document_path(skills_root, &record.relative_path);
        let content = std::fs::read_to_string(&skill_path).with_context(|| {
            format!(
                "failed to read skill recommendation file {}",
                skill_path.display()
            )
        })?;
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&metadata_relative_path, &content),
            excerpt: excerpt_skill(&content),
            record,
        });
    }
    Ok(candidates)
}

fn collect_filesystem_skill_candidates(skills_root: &Path) -> Result<Vec<SkillCandidateInput>> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;

    let mut candidates = Vec::new();
    for path in files {
        let relative_path = path
            .strip_prefix(skills_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "failed to read skill recommendation file {}",
                path.display()
            )
        })?;
        let derived = derive_skill_metadata(&relative_path, &content);
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&relative_path, &content),
            excerpt: excerpt_skill(&content),
            record: synthetic_skill_variant_record(&relative_path, &derived),
        });
    }

    Ok(candidates)
}

fn synthetic_skill_variant_record(
    relative_path: &str,
    derived: &crate::history::DerivedSkillMetadata,
) -> SkillVariantRecord {
    let normalized = relative_path.replace('\\', "/");
    let now = crate::history::now_ts();

    SkillVariantRecord {
        variant_id: format!("fs:{normalized}"),
        skill_name: if derived.skill_name.is_empty() {
            "skill".to_string()
        } else {
            derived.skill_name.clone()
        },
        variant_name: derived.variant_name.clone(),
        relative_path: normalized,
        parent_variant_id: None,
        version: "v1.0".to_string(),
        context_tags: derived.context_tags.clone(),
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        status: "active".to_string(),
        last_used_at: None,
        created_at: now,
        updated_at: now,
    }
}

pub(super) fn page_public_discovery_result(
    query: &str,
    context_tags: &[String],
    result: &SkillDiscoveryResult,
    cfg: &SkillRecommendationConfig,
    cursor: Option<&str>,
    limit: usize,
) -> Result<amux_protocol::SkillDiscoveryResultPublic> {
    let limit = limit.clamp(1, 100);
    let start_index = decode_page_cursor(cursor)?
        .as_deref()
        .and_then(|variant_id| {
            result
                .recommendations
                .iter()
                .position(|item| item.record.variant_id == variant_id)
                .map(|index| index + 1)
        })
        .unwrap_or(0);
    let page = result
        .recommendations
        .iter()
        .skip(start_index)
        .take(limit)
        .collect::<Vec<_>>();
    let next_cursor = if start_index + page.len() < result.recommendations.len() {
        page.last()
            .map(|recommendation| encode_page_cursor(&recommendation.record.variant_id))
    } else {
        None
    };

    let top_skill_name = result
        .recommendations
        .first()
        .map(|recommendation| recommendation.record.skill_name.as_str());

    Ok(amux_protocol::SkillDiscoveryResultPublic {
        query: query.to_string(),
        normalized_intent: query.to_string(),
        required: !matches!(result.recommended_action, SkillRecommendationAction::None),
        confidence_tier: confidence_label(result.confidence).to_string(),
        recommended_action: recommended_action_label(result.recommended_action, top_skill_name),
        requires_approval: false,
        mesh_state: "fresh".to_string(),
        rationale: result
            .recommendations
            .first()
            .map(|recommendation| split_reasons(&recommendation.reason))
            .unwrap_or_default(),
        capability_family: context_tags.to_vec(),
        explicit_rationale_required: false,
        workspace_tags: context_tags.to_vec(),
        candidates: page
            .into_iter()
            .map(
                |recommendation| amux_protocol::SkillDiscoveryCandidatePublic {
                    variant_id: recommendation.record.variant_id.clone(),
                    skill_name: recommendation.record.skill_name.clone(),
                    variant_name: recommendation.record.variant_name.clone(),
                    relative_path: recommendation.record.relative_path.clone(),
                    status: recommendation.record.status.clone(),
                    score: recommendation.score,
                    confidence_tier: candidate_confidence_label(recommendation.score, cfg)
                        .to_string(),
                    reasons: split_reasons(&recommendation.reason),
                    matched_intents: vec![query.to_string()],
                    matched_trigger_phrases: recommendation.metadata.triggers.clone(),
                    context_tags: recommendation.record.context_tags.clone(),
                    risk_level: if recommendation.metadata.built_in {
                        "low".to_string()
                    } else {
                        "medium".to_string()
                    },
                    trust_tier: if recommendation.metadata.built_in {
                        "trusted_builtin".to_string()
                    } else {
                        "trusted_local".to_string()
                    },
                    source_kind: if recommendation.record.relative_path.contains("generated/") {
                        "generated".to_string()
                    } else {
                        "builtin".to_string()
                    },
                    recommended_action: recommended_action_label(
                        result.recommended_action,
                        Some(recommendation.record.skill_name.as_str()),
                    ),
                    use_count: recommendation.record.use_count,
                    success_count: recommendation.record.success_count,
                    failure_count: recommendation.record.failure_count,
                },
            )
            .collect(),
        next_cursor,
    })
}

fn encode_page_cursor(variant_id: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(variant_id.as_bytes());
    format!("{DISCOVERY_CURSOR_PREFIX}{encoded}")
}

fn decode_page_cursor(cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let payload = cursor
        .strip_prefix(DISCOVERY_CURSOR_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid discovery cursor"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| anyhow::anyhow!("invalid discovery cursor: {error}"))?;
    let value = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("invalid discovery cursor: {error}"))?;
    Ok(Some(value))
}

fn confidence_label(value: SkillRecommendationConfidence) -> &'static str {
    match value {
        SkillRecommendationConfidence::Strong => "strong",
        SkillRecommendationConfidence::Weak => "weak",
        SkillRecommendationConfidence::None => "none",
    }
}

fn action_label(value: SkillRecommendationAction) -> &'static str {
    match value {
        SkillRecommendationAction::ReadSkill => "read_skill",
        SkillRecommendationAction::None => "none",
    }
}

fn recommended_action_label(
    action: SkillRecommendationAction,
    top_skill_name: Option<&str>,
) -> String {
    match (action, top_skill_name) {
        (SkillRecommendationAction::ReadSkill, Some(skill_name)) => {
            format!("read_skill {skill_name}")
        }
        _ => action_label(action).to_string(),
    }
}

fn candidate_confidence_label(score: f64, cfg: &SkillRecommendationConfig) -> &'static str {
    if score >= cfg.strong_match_threshold {
        "strong"
    } else if score >= cfg.weak_match_threshold {
        "weak"
    } else {
        "none"
    }
}

fn split_reasons(reason: &str) -> Vec<String> {
    let parts = reason
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if let Some(rest) = value.strip_prefix("matched request terms ") {
                format!("matched {rest}")
            } else if let Some(rest) = value.strip_prefix("matched workspace tags ") {
                format!("workspace {rest}")
            } else if value.starts_with("historical success ") {
                reason_usage_summary(value)
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        vec![reason.to_string()]
    } else {
        parts
    }
}

fn reason_usage_summary(value: &str) -> String {
    let words = value.split_whitespace().collect::<Vec<_>>();
    let uses = words
        .iter()
        .position(|word| *word == "across")
        .and_then(|index| words.get(index + 1))
        .and_then(|count| count.parse::<u32>().ok());
    let success_percent = words
        .get(2)
        .map(|value| value.trim_end_matches('%'))
        .and_then(|value| value.parse::<u32>().ok());

    match (uses, Some(success_percent).flatten()) {
        (Some(uses), Some(success_percent)) => {
            let successes = ((uses as f64) * (success_percent as f64 / 100.0)).round() as u32;
            format!("{successes}/{uses} successful uses")
        }
        _ => value.to_string(),
    }
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
