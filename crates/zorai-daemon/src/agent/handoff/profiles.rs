//! Default specialist profiles and capability-based matching.

use rand::distributions::{Distribution, WeightedIndex};
use rand::thread_rng;

use crate::agent::handoff::audit::CapabilityScoreRow;
use crate::agent::types::RoutingConfig;

use super::{CapabilityTag, Proficiency, RoutingMethod, SpecialistProfile};

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingSelection {
    pub profile_idx: usize,
    pub routing_method: RoutingMethod,
    pub routing_score: f64,
    pub fallback_used: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearnedRoutingWeight {
    pub profile_idx: usize,
    pub weight: f64,
}

/// Returns the built-in specialist profiles.
pub fn default_specialist_profiles() -> Vec<SpecialistProfile> {
    vec![
        SpecialistProfile {
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            role: "researcher".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "research".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "analysis".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "documentation".to_string(),
                    proficiency: Proficiency::Advanced,
                },
            ],
            tool_filter: Some(vec![
                "web_search".to_string(),
                "read_file".to_string(),
                "list_directory".to_string(),
                "search_codebase".to_string(),
                "broadcast_contribution".to_string(),
            ]),
            system_prompt_snippet: Some(
                "You are a research specialist. Focus on thorough investigation, \
                 source verification, and structured analysis. Provide evidence-based \
                 findings with clear citations."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "medical-research".to_string(),
            name: "Medical Research Specialist".to_string(),
            role: "medical-research".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "medical".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "evidence-synthesis".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "clinical-guidelines".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "patient-explanation".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a medical research specialist. Summarize evidence, compare \
                 guidelines, and explain findings clearly. Provide informational \
                 decision support only: do not diagnose, prescribe, or present \
                 output as a substitute for licensed clinical judgment. Flag \
                 uncertainty and escalate emergency-risk situations immediately."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "financial-analyst".to_string(),
            name: "Financial Analyst".to_string(),
            role: "financial-analyst".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "finance".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "modeling".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "risk-analysis".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "budgeting".to_string(),
                    proficiency: Proficiency::Competent,
                },
                CapabilityTag {
                    tag: "market-analysis".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a financial analysis specialist. Build explicit assumptions, \
                 compare scenarios, and surface downside cases and uncertainty. \
                 Provide informational analysis only: do not claim fiduciary \
                 authority, guaranteed returns, or personalized licensed advice."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "legal-research".to_string(),
            name: "Legal Research Specialist".to_string(),
            role: "legal-research".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "legal".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "contract-analysis".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "issue-spotting".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "jurisdictional-comparison".to_string(),
                    proficiency: Proficiency::Competent,
                },
                CapabilityTag {
                    tag: "compliance".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a legal research specialist. Summarize authorities, compare \
                 jurisdictions, and spot contract or compliance issues with clear \
                 uncertainty notes. Provide informational research only: do not \
                 claim the final legal answer or present output as legal advice."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "backend-developer".to_string(),
            name: "Backend Developer".to_string(),
            role: "backend-developer".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "rust".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "backend".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "api-design".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "testing".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "database".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a backend development specialist. Write production-quality \
                 Rust code with proper error handling, tests, and documentation."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "frontend-developer".to_string(),
            name: "Frontend Developer".to_string(),
            role: "frontend-developer".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "typescript".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "react".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "frontend".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "css".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "testing".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a frontend development specialist. Build responsive, \
                 accessible React components with TypeScript strict mode compliance."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "art-director".to_string(),
            name: "Art Director".to_string(),
            role: "art-director".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "art-direction".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "branding".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "visual-design".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "critique".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "concept-development".to_string(),
                    proficiency: Proficiency::Expert,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are an art direction specialist. Generate multiple credible \
                 visual directions, articulate taste and style trade-offs, and \
                 maintain coherence across concepts, branding, and presentation."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "scientific-reviewer".to_string(),
            name: "Scientific Reviewer".to_string(),
            role: "scientific-reviewer".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "science".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "methodology".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "statistics".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "peer-review".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "evidence-synthesis".to_string(),
                    proficiency: Proficiency::Advanced,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a scientific review specialist. Evaluate study design, \
                 evidence quality, methodology, and statistical reasoning. \
                 Separate strong findings from speculation and call out the key \
                 limitations that affect confidence."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "product-marketing-strategist".to_string(),
            name: "Product Marketing Strategist".to_string(),
            role: "product-marketing-strategist".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "marketing".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "positioning".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "messaging".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "go-to-market".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "product-strategy".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: Some(
                "You are a product marketing specialist. Clarify audience, \
                 positioning, messaging, and launch trade-offs. Tie recommendations \
                 to customer understanding, market context, and measurable outcomes."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "reviewer".to_string(),
            name: "Code Reviewer".to_string(),
            role: "reviewer".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "code-review".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "testing".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "architecture".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "documentation".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: Some(vec![
                "read_file".to_string(),
                "list_directory".to_string(),
                "search_codebase".to_string(),
                "broadcast_contribution".to_string(),
            ]),
            system_prompt_snippet: Some(
                "You are a code review specialist. Evaluate code quality, correctness, \
                 testing coverage, and architectural alignment. Provide actionable feedback."
                    .to_string(),
            ),
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
        SpecialistProfile {
            id: "generalist".to_string(),
            name: "Generalist".to_string(),
            role: "generalist".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "general".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "research".to_string(),
                    proficiency: Proficiency::Competent,
                },
                CapabilityTag {
                    tag: "coding".to_string(),
                    proficiency: Proficiency::Competent,
                },
                CapabilityTag {
                    tag: "debugging".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: None,
            escalation_chain: Vec::new(),
            is_builtin: true,
        },
    ]
}

/// Score a specialist profile against required capability tags.
///
/// Score = sum of matched tag proficiency weights / required_tags.len().
/// Returns `None` if `profiles` is empty.
/// Returns the index + score of the best-matching profile.
/// When top-2 candidates are within 10% of each other, the one with the
/// higher maximum single-tag proficiency wins (tie-break).
pub fn match_specialist(
    profiles: &[SpecialistProfile],
    required_tags: &[String],
    threshold: f64,
) -> Option<(usize, f64)> {
    if profiles.is_empty() || required_tags.is_empty() {
        return None;
    }

    let mut scored: Vec<(usize, f64, f64)> = profiles
        .iter()
        .enumerate()
        .map(|(idx, profile)| {
            let mut total_weight = 0.0_f64;
            let mut max_tag_weight = 0.0_f64;
            for req_tag in required_tags {
                if let Some(cap) = profile.capabilities.iter().find(|c| c.tag == *req_tag) {
                    let w = cap.proficiency.weight();
                    total_weight += w;
                    if w > max_tag_weight {
                        max_tag_weight = w;
                    }
                }
            }
            let score = total_weight / required_tags.len() as f64;
            (idx, score, max_tag_weight)
        })
        .collect();

    // Sort descending by score, then by max_tag_weight for tie-break.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
    });

    // If top-2 are within 10%, use max_tag_weight as tiebreaker.
    if scored.len() >= 2 {
        let (idx1, score1, max1) = scored[0];
        let (_idx2, score2, max2) = scored[1];
        if score1 > 0.0 && (score1 - score2).abs() / score1 < 0.10 {
            // Within 10% — prefer higher max tag proficiency.
            if max2 > max1 {
                // Swap: second candidate wins.
                let winner = scored[1];
                if winner.1 >= threshold {
                    return Some((winner.0, winner.1));
                }
            }
        }
        if score1 >= threshold {
            return Some((idx1, score1));
        }
        return None;
    }

    let (idx, score, _) = scored[0];
    if score >= threshold {
        Some((idx, score))
    } else {
        None
    }
}

fn compute_recency_decay(last_attempt_ms: Option<u64>, half_life_hours: f64, now_ms: u64) -> f64 {
    let Some(last_attempt_ms) = last_attempt_ms else {
        return 1.0;
    };
    if !half_life_hours.is_finite() || half_life_hours <= 0.0 || now_ms <= last_attempt_ms {
        return 1.0;
    }
    let elapsed_hours = (now_ms - last_attempt_ms) as f64 / 3_600_000.0;
    let decay_constant = std::f64::consts::LN_2 / half_life_hours;
    (-elapsed_hours * decay_constant).exp()
}

pub(crate) fn compute_learned_routing_weights(
    profiles: &[SpecialistProfile],
    required_tags: &[String],
    score_rows: &[CapabilityScoreRow],
    routing: &RoutingConfig,
    now_ms: u64,
) -> Vec<LearnedRoutingWeight> {
    if profiles.is_empty() || required_tags.is_empty() || !routing.enabled {
        return Vec::new();
    }

    let mut weights = Vec::new();
    for (profile_idx, profile) in profiles.iter().enumerate() {
        let mut aggregate_weight = 0.0_f64;
        let mut matched_any_tag = false;

        for required_tag in required_tags {
            let supports_tag = profile
                .capabilities
                .iter()
                .any(|cap| cap.tag == *required_tag);
            if !supports_tag {
                continue;
            }
            matched_any_tag = true;

            let learned = score_rows
                .iter()
                .find(|row| row.agent_id == profile.id && row.capability_tag == *required_tag);

            let tag_weight = if let Some(row) = learned {
                let attempts = row.attempts as f64;
                let successes = row.successes as f64;
                let bayesian_success_rate = (successes + routing.bayesian_alpha)
                    / (attempts + (2.0 * routing.bayesian_alpha));
                let confidence_factor = row.avg_confidence_score.clamp(0.0, 1.0);
                let recency_decay = compute_recency_decay(
                    row.last_attempt_ms,
                    routing.recency_decay_half_life_hours,
                    now_ms,
                );
                bayesian_success_rate * confidence_factor * recency_decay
            } else {
                0.25
            };

            aggregate_weight += tag_weight;
        }

        if matched_any_tag && aggregate_weight > 0.0 {
            weights.push(LearnedRoutingWeight {
                profile_idx,
                weight: aggregate_weight,
            });
        }
    }

    weights.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    weights
}

#[cfg(test)]
pub(crate) fn select_learned_specialist(
    learned_weights: &[LearnedRoutingWeight],
    confidence_threshold: f64,
) -> Option<(usize, f64)> {
    if learned_weights.is_empty() || !confidence_threshold.is_finite() {
        return None;
    }

    let eligible: Vec<&LearnedRoutingWeight> = learned_weights
        .iter()
        .filter(|entry| entry.weight >= confidence_threshold)
        .collect();

    match eligible.len() {
        0 => None,
        1 => {
            let only = eligible[0];
            Some((only.profile_idx, only.weight))
        }
        _ => {
            let weights: Vec<f64> = eligible.iter().map(|entry| entry.weight).collect();
            let dist = WeightedIndex::new(&weights).ok()?;
            let selected = eligible[dist.sample(&mut thread_rng())];
            Some((selected.profile_idx, selected.weight))
        }
    }
}

/// Select a specialist using a probabilistic pass across all non-zero matches,
/// with deterministic fallback when no candidate clears the threshold.
pub fn select_specialist(
    profiles: &[SpecialistProfile],
    required_tags: &[String],
    threshold: f64,
) -> Option<RoutingSelection> {
    if profiles.is_empty() || required_tags.is_empty() {
        return None;
    }

    let scored: Vec<(usize, f64)> = profiles
        .iter()
        .enumerate()
        .map(|(idx, profile)| {
            let total_weight = required_tags
                .iter()
                .filter_map(|req_tag| {
                    profile
                        .capabilities
                        .iter()
                        .find(|cap| cap.tag == *req_tag)
                        .map(|cap| cap.proficiency.weight())
                })
                .sum::<f64>();
            let score = total_weight / required_tags.len() as f64;
            (idx, score)
        })
        .collect();

    let probabilistic_candidates: Vec<(usize, f64)> = scored
        .iter()
        .copied()
        .filter(|(_, score)| *score > 0.0)
        .collect();

    if probabilistic_candidates.len() >= 2 {
        let weights: Vec<f64> = probabilistic_candidates
            .iter()
            .map(|(_, score)| *score)
            .collect();
        if let Ok(dist) = WeightedIndex::new(&weights) {
            let selected = probabilistic_candidates[dist.sample(&mut thread_rng())];
            if selected.1 >= threshold {
                return Some(RoutingSelection {
                    profile_idx: selected.0,
                    routing_method: RoutingMethod::Probabilistic,
                    routing_score: selected.1,
                    fallback_used: false,
                });
            }
        }
    }

    if let Some((idx, score)) = match_specialist(profiles, required_tags, threshold) {
        return Some(RoutingSelection {
            profile_idx: idx,
            routing_method: RoutingMethod::Deterministic,
            routing_score: score,
            fallback_used: false,
        });
    }

    Some(RoutingSelection {
        profile_idx: profiles.len().saturating_sub(1),
        routing_method: RoutingMethod::Deterministic,
        routing_score: 0.0,
        fallback_used: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profiles_include_expanded_specialist_catalog() {
        let profiles = default_specialist_profiles();
        assert_eq!(
            profiles.len(),
            11,
            "expected expanded default specialist profile catalog"
        );
    }

    #[test]
    fn each_profile_has_at_least_2_capabilities() {
        let profiles = default_specialist_profiles();
        for p in &profiles {
            assert!(
                p.capabilities.len() >= 2,
                "profile '{}' has fewer than 2 capability tags",
                p.id
            );
        }
    }

    #[test]
    fn match_researcher_for_research_analysis() {
        let profiles = default_specialist_profiles();
        let tags = vec!["research".to_string(), "analysis".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some(), "should match a specialist");
        let (idx, score) = result.unwrap();
        assert_eq!(profiles[idx].id, "researcher");
        assert!(
            score > 0.9,
            "researcher should score ~1.0 for research+analysis, got {score}"
        );
    }

    #[test]
    fn match_backend_for_rust_backend() {
        let profiles = default_specialist_profiles();
        let tags = vec!["rust".to_string(), "backend".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "backend-developer");
    }

    #[test]
    fn match_frontend_for_react_frontend() {
        let profiles = default_specialist_profiles();
        let tags = vec!["react".to_string(), "frontend".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "frontend-developer");
    }

    #[test]
    fn match_reviewer_for_code_review() {
        let profiles = default_specialist_profiles();
        let tags = vec!["code-review".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "reviewer");
    }

    #[test]
    fn match_medical_specialist_for_medical_evidence_work() {
        let profiles = default_specialist_profiles();
        let tags = vec!["medical".to_string(), "clinical-guidelines".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, score) = result.unwrap();
        assert_eq!(profiles[idx].id, "medical-research");
        assert!(
            score >= 0.8,
            "medical specialist should score strongly, got {score}"
        );
    }

    #[test]
    fn match_financial_specialist_for_finance_and_risk() {
        let profiles = default_specialist_profiles();
        let tags = vec!["finance".to_string(), "risk-analysis".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "financial-analyst");
    }

    #[test]
    fn match_legal_specialist_for_contract_analysis() {
        let profiles = default_specialist_profiles();
        let tags = vec!["legal".to_string(), "contract-analysis".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "legal-research");
    }

    #[test]
    fn match_art_director_for_visual_concept_work() {
        let profiles = default_specialist_profiles();
        let tags = vec![
            "art-direction".to_string(),
            "concept-development".to_string(),
        ];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "art-director");
    }

    #[test]
    fn match_scientific_reviewer_for_methodology_and_statistics() {
        let profiles = default_specialist_profiles();
        let tags = vec!["methodology".to_string(), "statistics".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "scientific-reviewer");
    }

    #[test]
    fn match_product_marketing_strategist_for_positioning_work() {
        let profiles = default_specialist_profiles();
        let tags = vec!["marketing".to_string(), "positioning".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "product-marketing-strategist");
    }

    #[test]
    fn match_generalist_as_fallback() {
        let profiles = default_specialist_profiles();
        // Tags that no specialist is expert at -- generalist should win as
        // best option or be the only one above threshold.
        let tags = vec!["general".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(profiles[idx].id, "generalist");
    }

    #[test]
    fn match_returns_none_for_empty_profiles() {
        let tags = vec!["research".to_string()];
        let result = match_specialist(&[], &tags, 0.3);
        assert!(result.is_none(), "should return None for empty profiles");
    }

    #[test]
    fn proficiency_weights_are_correct() {
        assert!((Proficiency::Expert.weight() - 1.0).abs() < f64::EPSILON);
        assert!((Proficiency::Advanced.weight() - 0.75).abs() < f64::EPSILON);
        assert!((Proficiency::Competent.weight() - 0.5).abs() < f64::EPSILON);
        assert!((Proficiency::Familiar.weight() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn ambiguous_match_prefers_higher_max_tag() {
        // Construct two profiles that score within 10% of each other.
        let profile_a = SpecialistProfile {
            id: "a".to_string(),
            name: "A".to_string(),
            role: "a".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "x".to_string(),
                    proficiency: Proficiency::Advanced,
                },
                CapabilityTag {
                    tag: "y".to_string(),
                    proficiency: Proficiency::Advanced,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: None,
            escalation_chain: Vec::new(),
            is_builtin: false,
        };
        let profile_b = SpecialistProfile {
            id: "b".to_string(),
            name: "B".to_string(),
            role: "b".to_string(),
            capabilities: vec![
                CapabilityTag {
                    tag: "x".to_string(),
                    proficiency: Proficiency::Expert,
                },
                CapabilityTag {
                    tag: "y".to_string(),
                    proficiency: Proficiency::Competent,
                },
            ],
            tool_filter: None,
            system_prompt_snippet: None,
            escalation_chain: Vec::new(),
            is_builtin: false,
        };
        // Both score (0.75+0.75)/2=0.75 vs (1.0+0.5)/2=0.75 -- exactly tied.
        // Profile B has higher max tag proficiency (1.0 vs 0.75).
        let profiles = vec![profile_a, profile_b];
        let tags = vec!["x".to_string(), "y".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(
            profiles[idx].id, "b",
            "should prefer B with higher max-tag proficiency"
        );
    }

    #[test]
    fn below_threshold_returns_none() {
        let profiles = default_specialist_profiles();
        // Tags that no profile matches well.
        let tags = vec!["quantum-physics".to_string(), "neurosurgery".to_string()];
        let result = match_specialist(&profiles, &tags, 0.3);
        assert!(result.is_none(), "should return None when below threshold");
    }

    #[test]
    fn select_specialist_uses_generalist_fallback_for_zero_match() {
        let profiles = default_specialist_profiles();
        let tags = vec!["quantum-physics".to_string(), "neurosurgery".to_string()];
        let result = select_specialist(&profiles, &tags, 0.3).expect("selection result");
        assert_eq!(profiles[result.profile_idx].id, "generalist");
        assert_eq!(result.routing_method, RoutingMethod::Deterministic);
        assert!(result.fallback_used);
        assert!((result.routing_score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn select_specialist_returns_probabilistic_when_multiple_candidates_clear_threshold() {
        let profiles = default_specialist_profiles();
        let tags = vec!["research".to_string()];
        let result = select_specialist(&profiles, &tags, 0.3).expect("selection result");
        assert_eq!(result.routing_method, RoutingMethod::Probabilistic);
        assert!(result.routing_score >= 0.5);
        assert!(!result.fallback_used);
        assert!(matches!(
            profiles[result.profile_idx].id.as_str(),
            "researcher" | "generalist"
        ));
    }

    #[test]
    fn compute_learned_routing_weights_prefers_stronger_historical_agent() {
        let profiles = default_specialist_profiles();
        let routing = RoutingConfig::default();
        let rows = vec![
            CapabilityScoreRow {
                agent_id: "researcher".to_string(),
                capability_tag: "research".to_string(),
                attempts: 10,
                successes: 9,
                failures: 1,
                partials: 0,
                last_attempt_ms: Some(1_000_000),
                avg_confidence_score: 0.9,
                total_tokens_used: 1000,
            },
            CapabilityScoreRow {
                agent_id: "generalist".to_string(),
                capability_tag: "research".to_string(),
                attempts: 10,
                successes: 4,
                failures: 6,
                partials: 0,
                last_attempt_ms: Some(1_000_000),
                avg_confidence_score: 0.6,
                total_tokens_used: 1000,
            },
        ];
        let tags = vec!["research".to_string()];

        let weights = compute_learned_routing_weights(&profiles, &tags, &rows, &routing, 1_000_000);
        assert!(!weights.is_empty());
        let top = &weights[0];
        assert_eq!(profiles[top.profile_idx].id, "researcher");
        assert!(top.weight > weights[1].weight);
    }

    #[test]
    fn compute_learned_routing_weights_uses_cold_start_prior() {
        let profiles = default_specialist_profiles();
        let routing = RoutingConfig::default();
        let tags = vec!["research".to_string()];

        let weights = compute_learned_routing_weights(&profiles, &tags, &[], &routing, 1_000_000);
        assert!(weights
            .iter()
            .any(|entry| profiles[entry.profile_idx].id == "researcher"));
        let researcher = weights
            .iter()
            .find(|entry| profiles[entry.profile_idx].id == "researcher")
            .expect("researcher weight");
        assert!((researcher.weight - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn routing_worker_produces_probabilistic_snapshot() {
        let profiles = default_specialist_profiles();
        let routing = RoutingConfig::default();
        let rows = vec![
            CapabilityScoreRow {
                agent_id: "researcher".to_string(),
                capability_tag: "research".to_string(),
                attempts: 20,
                successes: 18,
                failures: 2,
                partials: 0,
                last_attempt_ms: Some(1_000_000),
                avg_confidence_score: 0.92,
                total_tokens_used: 2000,
            },
            CapabilityScoreRow {
                agent_id: "generalist".to_string(),
                capability_tag: "research".to_string(),
                attempts: 20,
                successes: 10,
                failures: 10,
                partials: 0,
                last_attempt_ms: Some(1_000_000),
                avg_confidence_score: 0.65,
                total_tokens_used: 2000,
            },
        ];
        let morphogenesis = vec![crate::agent::morphogenesis::types::MorphogenesisAffinity {
            agent_id: "researcher".to_string(),
            domain: "research".to_string(),
            affinity_score: 0.88,
            task_count: 14,
            success_count: 13,
            failure_count: 1,
            last_updated_ms: 1_000_000,
        }];

        let snapshot = crate::agent::background_workers::domain_routing::build_routing_snapshot(
            &profiles,
            &["research".to_string()],
            &rows,
            &morphogenesis,
            &routing,
            1_000_000,
        );

        assert_eq!(snapshot.required_tags, vec!["research".to_string()]);
        assert_eq!(snapshot.ranked_candidates[0].profile_id, "researcher");
        assert!(
            snapshot.ranked_candidates[0].final_weight > snapshot.ranked_candidates[1].final_weight
        );
        assert!(snapshot.ranked_candidates[0].morphogenesis_affinity >= 0.88);
    }

    #[test]
    fn compute_learned_routing_weights_converge_toward_higher_success_history() {
        let profiles = default_specialist_profiles();
        let routing = RoutingConfig::default();
        let tags = vec!["research".to_string()];
        let now_ms = 10_000_000;

        let cold = compute_learned_routing_weights(&profiles, &tags, &[], &routing, now_ms);
        let cold_researcher = cold
            .iter()
            .find(|entry| profiles[entry.profile_idx].id == "researcher")
            .expect("cold researcher weight")
            .weight;
        let cold_generalist = cold
            .iter()
            .find(|entry| profiles[entry.profile_idx].id == "generalist")
            .expect("cold generalist weight")
            .weight;
        assert!((cold_researcher - cold_generalist).abs() < f64::EPSILON);

        let converged_rows = vec![
            CapabilityScoreRow {
                agent_id: "researcher".to_string(),
                capability_tag: "research".to_string(),
                attempts: 100,
                successes: 92,
                failures: 8,
                partials: 0,
                last_attempt_ms: Some(now_ms),
                avg_confidence_score: 0.95,
                total_tokens_used: 20_000,
            },
            CapabilityScoreRow {
                agent_id: "generalist".to_string(),
                capability_tag: "research".to_string(),
                attempts: 100,
                successes: 18,
                failures: 82,
                partials: 0,
                last_attempt_ms: Some(now_ms),
                avg_confidence_score: 0.55,
                total_tokens_used: 20_000,
            },
        ];

        let converged =
            compute_learned_routing_weights(&profiles, &tags, &converged_rows, &routing, now_ms);
        let top = &converged[0];
        let runner_up = &converged[1];
        assert_eq!(profiles[top.profile_idx].id, "researcher");
        assert_eq!(profiles[runner_up.profile_idx].id, "generalist");
        assert!(top.weight > runner_up.weight * 2.0);
        assert!(top.weight > cold_researcher);
    }

    #[test]
    fn morphogenesis_updates_affinity_from_success_and_decay() {
        let updated = crate::agent::morphogenesis::affinity_tracker::apply_outcome(
            None,
            "researcher",
            "research",
            crate::agent::morphogenesis::types::MorphogenesisOutcome::Success,
            1_000,
        );
        assert_eq!(updated.agent_id, "researcher");
        assert_eq!(updated.domain, "research");
        assert!(updated.affinity_score > 0.1);
        assert_eq!(updated.task_count, 1);
        assert_eq!(updated.success_count, 1);

        let decayed = crate::agent::morphogenesis::affinity_tracker::apply_decay(
            updated.clone(),
            3 * 24 * 60 * 60 * 1000 + 1_000,
            0.01,
        );
        assert!(decayed.affinity_score < updated.affinity_score);
        assert_eq!(decayed.task_count, updated.task_count);
    }

    #[test]
    fn select_learned_specialist_returns_none_when_no_candidate_clears_threshold() {
        let learned = vec![
            LearnedRoutingWeight {
                profile_idx: 0,
                weight: 0.2,
            },
            LearnedRoutingWeight {
                profile_idx: 1,
                weight: 0.1,
            },
        ];

        assert!(select_learned_specialist(&learned, 0.3).is_none());
    }

    #[test]
    fn select_learned_specialist_uses_only_threshold_eligible_candidates() {
        let learned = vec![
            LearnedRoutingWeight {
                profile_idx: 10,
                weight: 0.6,
            },
            LearnedRoutingWeight {
                profile_idx: 20,
                weight: 0.4,
            },
            LearnedRoutingWeight {
                profile_idx: 99,
                weight: 0.2,
            },
        ];

        for _ in 0..200 {
            let (profile_idx, weight) =
                select_learned_specialist(&learned, 0.3).expect("eligible candidate");
            assert!(matches!(profile_idx, 10 | 20));
            assert!(weight >= 0.3);
        }
    }

    #[test]
    fn select_learned_specialist_with_single_eligible_candidate_is_stable() {
        let learned = vec![
            LearnedRoutingWeight {
                profile_idx: 3,
                weight: 0.8,
            },
            LearnedRoutingWeight {
                profile_idx: 4,
                weight: 0.1,
            },
        ];

        let selected = select_learned_specialist(&learned, 0.3).expect("single eligible");
        assert_eq!(selected.0, 3);
        assert_eq!(selected.1, 0.8);
    }

    #[test]
    fn default_profiles_are_builtin() {
        let profiles = default_specialist_profiles();
        for p in &profiles {
            assert!(
                p.is_builtin,
                "default profile '{}' should be marked builtin",
                p.id
            );
        }
    }
}
