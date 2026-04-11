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

use super::types::{SkillMeshConfidenceBand, SkillMeshNextStep, SkillMeshPolicyDecision};

pub(crate) async fn discover_local_skills_via_mesh(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    let records = history.list_skill_variants(None, 512).await?;
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
        let reason = if matched_workspace_tags.is_empty() {
            format!("mesh synthetic match {}", matched_terms.join(", "))
        } else {
            format!(
                "mesh synthetic match {}; workspace {}",
                matched_terms.join(", "),
                matched_workspace_tags.join(", ")
            )
        };
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
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn score_history(record: &SkillVariantRecord) -> f64 {
    let use_score = (record.use_count as f64 / 8.0).clamp(0.0, 1.0);
    (record.success_rate() * 0.65) + (use_score * 0.35)
}

pub fn policy_decision_for_legacy_discovery(
    result: &crate::agent::skill_recommendation::SkillDiscoveryResult,
) -> SkillMeshPolicyDecision {
    let top_recommendation = result.recommendations.first();
    let recommended_skill = top_recommendation.map(|item| item.record.skill_name.clone());
    let read_skill_identifier = top_recommendation.map(|item| item.record.variant_id.clone());

    let confidence_band = match result.confidence {
        crate::agent::skill_recommendation::SkillRecommendationConfidence::Strong => {
            SkillMeshConfidenceBand::Strong
        }
        crate::agent::skill_recommendation::SkillRecommendationConfidence::Weak => {
            SkillMeshConfidenceBand::Weak
        }
        crate::agent::skill_recommendation::SkillRecommendationConfidence::None => {
            SkillMeshConfidenceBand::None
        }
    };

    match confidence_band {
        SkillMeshConfidenceBand::Strong => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::ReadSkill,
            recommended_action: recommended_skill
                .as_deref()
                .map(|skill| format!("read_skill {skill}"))
                .unwrap_or_else(|| SkillMeshNextStep::JustifySkillSkip.as_str().to_string()),
            recommended_skill,
            read_skill_identifier,
            requires_approval: false,
        },
        SkillMeshConfidenceBand::Weak => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::ChooseOrBypass,
            recommended_action: recommended_skill
                .as_deref()
                .map(|skill| format!("read_skill {skill}"))
                .unwrap_or_else(|| SkillMeshNextStep::JustifySkillSkip.as_str().to_string()),
            recommended_skill,
            read_skill_identifier,
            requires_approval: false,
        },
        SkillMeshConfidenceBand::None => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::JustifySkillSkip,
            recommended_action: SkillMeshNextStep::JustifySkillSkip.as_str().to_string(),
            recommended_skill: None,
            read_skill_identifier: None,
            requires_approval: false,
        },
    }
}
