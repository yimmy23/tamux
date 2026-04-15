use crate::agent::skill_recommendation::types::{
    CandidateScore, GraphSkillSignal, SkillCandidateInput, SkillDiscoveryResult,
    SkillRecommendation, SkillRecommendationAction, SkillRecommendationConfidence,
};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::SkillVariantRecord;
use std::collections::BTreeSet;

const MAX_USE_SCORE: f64 = 8.0;
const RECENCY_DAY_SECS: u64 = 86_400;
const MAX_LEXICAL_QUERY_TOKENS: usize = 8;
const PROCESS_INTENT_TOKENS: &[&str] = &[
    "architect",
    "behavior",
    "brainstorm",
    "debug",
    "design",
    "feature",
    "guide",
    "implement",
    "investigate",
    "modify",
    "plan",
    "playbook",
    "refactor",
    "review",
    "workflow",
    "synthes",
];

pub(super) fn rank_skill_candidates(
    candidates: Vec<SkillCandidateInput>,
    query: &str,
    workspace_tags: &[String],
    graph_signals: &std::collections::HashMap<String, GraphSkillSignal>,
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
        .map(|candidate| {
            score_candidate(candidate, &query_tokens, workspace_tags, graph_signals, cfg)
        })
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
                .graph_score
                .partial_cmp(&left.graph_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
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
    graph_signals: &std::collections::HashMap<String, GraphSkillSignal>,
    cfg: &SkillRecommendationConfig,
) -> CandidateScore {
    let search_tokens = tokenize(&candidate.metadata.search_text);
    let matched_terms = query_tokens
        .iter()
        .filter(|token| search_tokens.contains(token.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let lexical_denominator = query_tokens.len().min(MAX_LEXICAL_QUERY_TOKENS).max(1);
    let lexical_overlap =
        matched_terms.len().min(lexical_denominator) as f64 / lexical_denominator as f64;

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
    let process_bonus = process_skill_match_bonus(&candidate.metadata, query_tokens);
    let built_in_bonus = if candidate.metadata.built_in {
        0.02
    } else {
        0.0
    };
    let graph_signal = graph_signals
        .get(&candidate.record.variant_id)
        .copied()
        .unwrap_or_default();
    let graph_score = graph_signal.score.clamp(0.0, 10.0) / 10.0;
    let novelty_score = if graph_signal.distance > 1 {
        (f64::from(graph_signal.distance.saturating_sub(1)) / 4.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let score = (lexical_overlap * 0.62)
        + (workspace_overlap * 0.16)
        + (history_score * 0.20)
        + (recency_score * 0.06)
        + (graph_score * 0.04)
        + (novelty_score * cfg.novelty_distance_weight)
        + lifecycle_bonus
        + process_bonus
        + built_in_bonus;

    CandidateScore {
        recommendation: SkillRecommendation {
            reason: build_reason(
                &candidate.record,
                &matched_terms,
                &matched_workspace_tags,
                lexical_overlap,
                workspace_overlap,
                graph_score,
                graph_signal.distance,
            ),
            score: score.clamp(0.0, 1.0),
            excerpt: candidate.excerpt,
            metadata: candidate.metadata,
            record: candidate.record,
        },
        graph_score,
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
    graph_score: f64,
    novelty_distance: u8,
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
    if graph_score > 0.0 {
        reasons.push(format!("graph affinity {:.0}%", graph_score * 100.0));
    }
    if novelty_distance > 1 {
        reasons.push(format!("novel graph path {} hops", novelty_distance));
    }
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
        SkillRecommendationConfidence::Strong => SkillRecommendationAction::ReadSkill,
        SkillRecommendationConfidence::Weak => SkillRecommendationAction::ReadSkill,
        SkillRecommendationConfidence::None => SkillRecommendationAction::None,
    }
}

fn process_skill_match_bonus(
    metadata: &crate::agent::skill_recommendation::types::SkillDocumentMetadata,
    query_tokens: &BTreeSet<String>,
) -> f64 {
    let intent_tokens = process_intent_tokens(metadata);
    if intent_tokens.is_empty() {
        return 0.0;
    }

    let process_token_count = intent_tokens
        .iter()
        .filter(|token| PROCESS_INTENT_TOKENS.contains(&token.as_str()))
        .count();
    let non_process_token_count = intent_tokens.len().saturating_sub(process_token_count);
    if process_token_count < 3 || process_token_count < non_process_token_count {
        return 0.0;
    }

    let matched_process_terms = query_tokens
        .iter()
        .filter(|token| {
            intent_tokens.contains(token.as_str())
                && PROCESS_INTENT_TOKENS.contains(&token.as_str())
        })
        .count();
    if matched_process_terms < 2 {
        return 0.0;
    }

    let extra_matches = matched_process_terms.saturating_sub(2) as f64;
    (0.18 + (extra_matches * 0.07)).min(0.25)
}

fn process_intent_tokens(
    metadata: &crate::agent::skill_recommendation::types::SkillDocumentMetadata,
) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for value in &metadata.headings {
        tokens.extend(tokenize(value));
    }
    for value in &metadata.keywords {
        tokens.extend(tokenize(value));
    }
    for value in &metadata.triggers {
        tokens.extend(tokenize(value));
    }
    if let Some(summary) = metadata.summary.as_deref() {
        tokens.extend(tokenize(summary));
    }
    tokens
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
        .filter_map(|token| {
            let trimmed = token.trim_matches(|character: char| !character.is_ascii_alphanumeric());
            if trimmed.len() < 3
                || STOPWORDS
                    .iter()
                    .any(|stopword| trimmed.eq_ignore_ascii_case(stopword))
            {
                return None;
            }
            Some(normalize_token(trimmed))
        })
        .collect()
}

fn normalize_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("architect") {
        "architect".to_string()
    } else if lower.starts_with("audit") {
        "audit".to_string()
    } else if lower.starts_with("brainstorm") {
        "brainstorm".to_string()
    } else if lower.starts_with("debug") {
        "debug".to_string()
    } else if lower.starts_with("compile") || lower.starts_with("compil") {
        "build".to_string()
    } else if lower.starts_with("build") {
        "build".to_string()
    } else if lower.starts_with("design") {
        "design".to_string()
    } else if lower.starts_with("diff") {
        "diff".to_string()
    } else if lower == "err" || lower.starts_with("error") {
        "failure".to_string()
    } else if lower.starts_with("fail") {
        "failure".to_string()
    } else if lower.starts_with("patch") || lower.starts_with("fix") || lower.starts_with("repair")
    {
        "fix".to_string()
    } else if lower.starts_with("implement") {
        "implement".to_string()
    } else if lower.starts_with("investigat") {
        "investigate".to_string()
    } else if lower.starts_with("modif") {
        "modify".to_string()
    } else if lower.starts_with("behavio") {
        "behavior".to_string()
    } else if lower.starts_with("govern") {
        "governance".to_string()
    } else if lower.starts_with("orchestrat") {
        "orchestration".to_string()
    } else if lower.starts_with("plan") {
        "plan".to_string()
    } else if lower.starts_with("playbook") {
        "playbook".to_string()
    } else if lower.starts_with("refactor") {
        "refactor".to_string()
    } else if lower.starts_with("review") {
        "review".to_string()
    } else if lower.starts_with("safe") {
        "safety".to_string()
    } else if lower.starts_with("workflow") {
        "workflow".to_string()
    } else if lower.starts_with("synthes") {
        "synthes".to_string()
    } else if lower.starts_with("feature") {
        "feature".to_string()
    } else if lower.starts_with("guide") {
        "guide".to_string()
    } else {
        lower
    }
}
