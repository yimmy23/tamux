use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;

use crate::agent::skill_mesh::compiler::{
    compile_skill_document, SkillMeshCompileContext, SkillMeshCompileMode,
};
use crate::agent::skill_recommendation::{
    SkillDiscoveryResult, SkillDocumentMetadata, SkillRecommendation, SkillRecommendationAction,
    SkillRecommendationConfidence,
};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::{HistoryStore, SkillVariantRecord};

pub(crate) async fn discover_local_skills_via_mesh(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    let mut records = history.list_skill_variants(None, 512).await?;
    if records.is_empty() {
        crate::agent::skill_recommendation::sync_skill_catalog(history, skills_root).await?;
        records = history.list_skill_variants(None, 512).await?;
    }
    let query_tokens = tokenize(query);
    if !cfg.enabled || query_tokens.is_empty() || limit == 0 {
        return Ok(SkillDiscoveryResult::default());
    }

    let mut recommendations = Vec::new();
    for record in records {
        if matches!(record.status.as_str(), "archived" | "merged" | "draft") {
            continue;
        }
        if !record.relative_path.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        let (path, _) = crate::agent::skill_recommendation::resolve_skill_document_path(
            skills_root,
            &record.relative_path,
        );
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(compiled) = compile_skill_document(
            path.clone(),
            &content,
            SkillMeshCompileContext {
                mode: SkillMeshCompileMode::Deterministic,
                compile_version: 1,
                source_kind: if record.relative_path.contains("generated/") {
                    "generated".to_string()
                } else {
                    "builtin".to_string()
                },
                trust_tier: "trusted".to_string(),
                provenance: "mesh-local".to_string(),
                risk_level: "low".to_string(),
            },
        )
        .await
        else {
            continue;
        };

        let search_text = format!(
            "{} {} {} {}",
            compiled.synthetic_queries.join(" "),
            compiled.explicit_trigger_phrases.join(" "),
            compiled.capability_path.join(" "),
            compiled.summary.clone().unwrap_or_default()
        );
        let search_tokens = tokenize(&search_text);
        let matched_terms = query_tokens
            .iter()
            .filter(|token| search_tokens.contains(token.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let lexical_overlap = matched_terms.len() as f64 / query_tokens.len().max(1) as f64;
        let matched_workspace_tags = workspace_tags
            .iter()
            .filter(|tag| {
                compiled
                    .workspace_affinities
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(tag))
            })
            .cloned()
            .collect::<Vec<_>>();
        let workspace_overlap = if workspace_tags.is_empty() {
            0.0
        } else {
            matched_workspace_tags.len() as f64 / workspace_tags.len() as f64
        };
        let history_score = score_history(&record);
        let score =
            ((lexical_overlap * 0.72) + (workspace_overlap * 0.14) + (history_score * 0.14))
                .clamp(0.0, 1.0);
        if score < cfg.weak_match_threshold {
            continue;
        }

        let metadata = SkillDocumentMetadata {
            summary: compiled.summary.clone(),
            headings: Vec::new(),
            keywords: compiled.workspace_affinities.clone(),
            triggers: compiled.explicit_trigger_phrases.clone(),
            search_text,
            built_in: compiled.source_kind == "builtin",
        };
        let reason = build_reason(
            &record,
            &matched_terms,
            &matched_workspace_tags,
            lexical_overlap,
            workspace_overlap,
        );
        recommendations.push(SkillRecommendation {
            record,
            metadata,
            reason,
            excerpt: content.lines().take(8).collect::<Vec<_>>().join("\n"),
            score,
        });
    }

    recommendations.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut deduped = Vec::new();
    let mut seen_skill_names = BTreeSet::new();
    for recommendation in recommendations {
        if seen_skill_names.insert(recommendation.record.skill_name.clone()) {
            deduped.push(recommendation);
        }
        if deduped.len() >= limit {
            break;
        }
    }
    let mut recommendations = deduped;
    let top_score = recommendations
        .first()
        .map(|item| item.score)
        .unwrap_or_default();
    let confidence = if top_score >= cfg.strong_match_threshold {
        SkillRecommendationConfidence::Strong
    } else if top_score >= cfg.weak_match_threshold {
        SkillRecommendationConfidence::Weak
    } else {
        SkillRecommendationConfidence::None
    };
    let recommended_action = match confidence {
        SkillRecommendationConfidence::Strong if cfg.require_read_on_strong_match => {
            SkillRecommendationAction::ReadSkill
        }
        _ => SkillRecommendationAction::None,
    };
    if matches!(confidence, SkillRecommendationConfidence::None) {
        recommendations.clear();
    }

    Ok(SkillDiscoveryResult {
        recommendations,
        confidence,
        recommended_action,
    })
}

fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .map(str::trim)
        .filter(|token| token.len() >= 2)
        .map(normalize_token)
        .collect()
}

fn normalize_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("compile") || lower.starts_with("compil") {
        "build".to_string()
    } else if lower.starts_with("build") {
        "build".to_string()
    } else if lower == "err" || lower.starts_with("error") {
        "failure".to_string()
    } else if lower.starts_with("fail") {
        "failure".to_string()
    } else if lower.starts_with("patch") || lower.starts_with("fix") || lower.starts_with("repair")
    {
        "fix".to_string()
    } else {
        lower
    }
}

fn score_history(record: &SkillVariantRecord) -> f64 {
    let use_score = (record.use_count as f64 / 8.0).clamp(0.0, 1.0);
    (record.success_rate() * 0.65) + (use_score * 0.35)
}

fn build_reason(
    record: &SkillVariantRecord,
    matched_terms: &[String],
    matched_workspace_tags: &[String],
    lexical_overlap: f64,
    workspace_overlap: f64,
) -> String {
    let mut reasons = Vec::new();
    if !matched_terms.is_empty() {
        reasons.push(format!(
            "matched request terms {}",
            matched_terms.join(", ")
        ));
    }
    if !matched_workspace_tags.is_empty() {
        reasons.push(format!(
            "matched workspace tags {}",
            matched_workspace_tags.join(", ")
        ));
    }
    if reasons.is_empty() && lexical_overlap > 0.0 {
        reasons.push("partial lexical overlap with the request".to_string());
    }
    if workspace_overlap > 0.0 && matched_workspace_tags.is_empty() {
        reasons.push("partial workspace overlap".to_string());
    }
    reasons.push(format!(
        "historical success {:.0}% across {} uses",
        record.success_rate() * 100.0,
        record.use_count
    ));
    reasons.join("; ")
}
