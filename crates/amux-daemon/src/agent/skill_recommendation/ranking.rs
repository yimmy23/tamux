use crate::agent::skill_recommendation::types::{
    CandidateScore, SkillCandidateInput, SkillDiscoveryResult, SkillRecommendation,
    SkillRecommendationAction, SkillRecommendationConfidence,
};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::SkillVariantRecord;
use std::collections::BTreeSet;

const MAX_USE_SCORE: f64 = 8.0;
const RECENCY_DAY_SECS: u64 = 86_400;

pub(super) fn rank_skill_candidates(
    candidates: Vec<SkillCandidateInput>,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> SkillDiscoveryResult {
    if !cfg.enabled || limit == 0 {
        return SkillDiscoveryResult::default();
    }

    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return SkillDiscoveryResult::default();
    }

    let mut ranked = candidates
        .into_iter()
        .map(|candidate| score_candidate(candidate, &query_tokens, workspace_tags))
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| compare_candidates(left, right));

    let mut recommendations = Vec::new();
    let mut seen_families = BTreeSet::new();
    for candidate in ranked {
        if candidate.recommendation.score < cfg.weak_match_threshold {
            continue;
        }
        if seen_families.insert(candidate.recommendation.record.skill_name.clone()) {
            recommendations.push(candidate.recommendation);
        }
        if recommendations.len() >= limit {
            break;
        }
    }

    let top_score = recommendations
        .first()
        .map(|item| item.score)
        .unwrap_or_default();
    let confidence = confidence_for(top_score, cfg);
    if matches!(confidence, SkillRecommendationConfidence::None) {
        recommendations.clear();
    }

    SkillDiscoveryResult {
        recommended_action: action_for(confidence, cfg),
        confidence,
        recommendations,
    }
}

fn compare_candidates(left: &CandidateScore, right: &CandidateScore) -> std::cmp::Ordering {
    right
        .recommendation
        .score
        .partial_cmp(&left.recommendation.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            right
                .recommendation
                .record
                .success_count
                .cmp(&left.recommendation.record.success_count)
        })
        .then_with(|| {
            right
                .recommendation
                .record
                .use_count
                .cmp(&left.recommendation.record.use_count)
        })
        .then_with(|| {
            left.recommendation
                .record
                .relative_path
                .cmp(&right.recommendation.record.relative_path)
        })
}

fn score_candidate(
    candidate: SkillCandidateInput,
    query_tokens: &BTreeSet<String>,
    workspace_tags: &[String],
) -> CandidateScore {
    let search_tokens = tokenize(&candidate.metadata.search_text);
    let matched_terms = query_tokens
        .iter()
        .filter(|token| search_tokens.contains(token.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let lexical_overlap = matched_terms.len() as f64 / query_tokens.len() as f64;

    let matched_workspace_tags = workspace_tags
        .iter()
        .filter(|tag| {
            candidate
                .record
                .context_tags
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

    let history_score = score_history(&candidate.record);
    let recency_score = score_recency(&candidate.record);
    let lifecycle_bonus = lifecycle_bonus(&candidate.record);
    let built_in_bonus = if candidate.metadata.built_in {
        0.02
    } else {
        0.0
    };
    let score = (lexical_overlap * 0.62)
        + (workspace_overlap * 0.16)
        + (history_score * 0.20)
        + (recency_score * 0.06)
        + lifecycle_bonus
        + built_in_bonus;

    CandidateScore {
        recommendation: SkillRecommendation {
            reason: build_reason(
                &candidate.record,
                &matched_terms,
                &matched_workspace_tags,
                lexical_overlap,
                workspace_overlap,
            ),
            score: score.clamp(0.0, 1.0),
            excerpt: candidate.excerpt,
            metadata: candidate.metadata,
            record: candidate.record,
        },
    }
}

fn score_history(record: &SkillVariantRecord) -> f64 {
    let use_score = (record.use_count as f64 / MAX_USE_SCORE).clamp(0.0, 1.0);
    (record.success_rate() * 0.65) + (use_score * 0.35)
}

fn score_recency(record: &SkillVariantRecord) -> f64 {
    let Some(reference) = record.last_used_at else {
        return 0.0;
    };
    let age_secs = crate::history::now_ts().saturating_sub(reference);
    match age_secs {
        0..=RECENCY_DAY_SECS => 1.0,
        86_401..=604_800 => 0.85,
        604_801..=2_592_000 => 0.6,
        2_592_001..=7_776_000 => 0.35,
        _ => 0.15,
    }
}

fn lifecycle_bonus(record: &SkillVariantRecord) -> f64 {
    let status_bonus = match record.status.as_str() {
        "promoted-to-canonical" | "promoted_to_canonical" => 0.04,
        "proven" => 0.035,
        "active" => 0.03,
        "testing" => 0.015,
        "deprecated" => 0.005,
        _ => 0.0,
    };
    if record.is_canonical() {
        status_bonus + 0.01
    } else {
        status_bonus
    }
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

fn confidence_for(
    top_score: f64,
    cfg: &SkillRecommendationConfig,
) -> SkillRecommendationConfidence {
    if top_score >= cfg.strong_match_threshold {
        SkillRecommendationConfidence::Strong
    } else if top_score >= cfg.weak_match_threshold {
        SkillRecommendationConfidence::Weak
    } else {
        SkillRecommendationConfidence::None
    }
}

fn action_for(
    confidence: SkillRecommendationConfidence,
    cfg: &SkillRecommendationConfig,
) -> SkillRecommendationAction {
    match confidence {
        SkillRecommendationConfidence::Strong if cfg.require_read_on_strong_match => {
            SkillRecommendationAction::ReadSkill
        }
        SkillRecommendationConfidence::Strong | SkillRecommendationConfidence::Weak => {
            SkillRecommendationAction::JustifySkip
        }
        SkillRecommendationConfidence::None => SkillRecommendationAction::None,
    }
}

fn tokenize(input: &str) -> BTreeSet<String> {
    const STOPWORDS: &[&str] = &[
        "about",
        "after",
        "backend",
        "from",
        "into",
        "that",
        "this",
        "with",
        "workspace",
    ];

    input
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
        .map(|token| token.trim_matches(|character: char| !character.is_ascii_alphanumeric()))
        .filter(|token| {
            token.len() >= 3
                && !STOPWORDS
                    .iter()
                    .any(|stopword| token.eq_ignore_ascii_case(stopword))
        })
        .map(|token| token.to_ascii_lowercase())
        .collect()
}
