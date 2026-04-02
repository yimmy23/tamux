//! Default specialist profiles and capability-based matching.

use super::{CapabilityTag, HandoffEscalationRule, Proficiency, SpecialistProfile};

/// Returns the 5 built-in specialist profiles.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profiles_returns_5() {
        let profiles = default_specialist_profiles();
        assert_eq!(profiles.len(), 5, "expected exactly 5 default profiles");
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
