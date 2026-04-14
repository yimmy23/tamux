use crate::agent::operator_model::RiskTolerance;

use super::types::{Argument, CritiqueDirective, Decision, Resolution};

fn top_claims(argument: &Argument, limit: usize) -> Vec<String> {
    let mut points = argument.points.clone();
    points.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points
        .into_iter()
        .take(limit)
        .map(|point| point.claim)
        .collect()
}

pub(crate) fn recommended_modifications(argument: &Argument, limit: usize) -> Vec<String> {
    recommended_modifications_with_fallback_targets(argument, &[], limit)
}

pub(crate) fn recommended_modifications_with_fallback_targets(
    argument: &Argument,
    preferred_fallback_targets: &[String],
    limit: usize,
) -> Vec<String> {
    let normalized_targets = preferred_fallback_targets
        .iter()
        .map(|target| target.trim().to_ascii_lowercase())
        .filter(|target| !target.is_empty())
        .collect::<Vec<_>>();
    let mut points = argument.points.clone();
    points.sort_by(|a, b| {
        let a_fallback_match = a.evidence.iter().any(|evidence| {
            evidence
                .strip_prefix("fallback_match:")
                .map(|tool| {
                    let normalized = tool.trim().to_ascii_lowercase();
                    normalized_targets.is_empty()
                        || normalized_targets
                            .iter()
                            .any(|target| target == &normalized)
                })
                .unwrap_or(false)
        });
        let b_fallback_match = b.evidence.iter().any(|evidence| {
            evidence
                .strip_prefix("fallback_match:")
                .map(|tool| {
                    let normalized = tool.trim().to_ascii_lowercase();
                    normalized_targets.is_empty()
                        || normalized_targets
                            .iter()
                            .any(|target| target == &normalized)
                })
                .unwrap_or(false)
        });
        let a_tool_specific = a
            .evidence
            .iter()
            .any(|evidence| evidence.starts_with("tool_specific:"));
        let b_tool_specific = b
            .evidence
            .iter()
            .any(|evidence| evidence.starts_with("tool_specific:"));
        b_fallback_match
            .cmp(&a_fallback_match)
            .then_with(|| b_tool_specific.cmp(&a_tool_specific))
            .then_with(|| {
                b.weight
                    .partial_cmp(&a.weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    points
        .into_iter()
        .take(limit)
        .map(|point| point.claim)
        .collect()
}

pub(crate) fn directives_for_modifications(modifications: &[String]) -> Vec<CritiqueDirective> {
    let mut directives = Vec::new();
    for modification in modifications {
        let normalized = modification.trim().to_ascii_lowercase();
        if (normalized.contains("disable network access") || normalized.contains("disable network"))
            && !directives.contains(&CritiqueDirective::DisableNetwork)
        {
            directives.push(CritiqueDirective::DisableNetwork);
        }
        if (normalized.contains("enable sandboxing") || normalized.contains("enable sandbox"))
            && !directives.contains(&CritiqueDirective::EnableSandbox)
        {
            directives.push(CritiqueDirective::EnableSandbox);
        }
        if (normalized.contains("downgrade any yolo security level")
            || normalized.contains("downgrade security level"))
            && !directives.contains(&CritiqueDirective::DowngradeSecurityLevel)
        {
            directives.push(CritiqueDirective::DowngradeSecurityLevel);
        }
        if (normalized.contains("strip explicit messaging targets")
            || normalized.contains("strip explicit message targets")
            || normalized.contains("strip explicit messaging target"))
            && !directives.contains(&CritiqueDirective::StripExplicitMessagingTargets)
        {
            directives.push(CritiqueDirective::StripExplicitMessagingTargets);
        }
        if (normalized.contains("broadcast mentions")
            || normalized.contains("broadcast mention")
            || normalized.contains("@everyone")
            || normalized.contains("@here"))
            && !directives.contains(&CritiqueDirective::StripBroadcastMentions)
        {
            directives.push(CritiqueDirective::StripBroadcastMentions);
        }
        if (normalized.contains("narrow the sensitive file path")
            || normalized.contains("narrow any sensitive file path")
            || normalized.contains("minimal basename"))
            && !directives.contains(&CritiqueDirective::NarrowSensitiveFilePath)
        {
            directives.push(CritiqueDirective::NarrowSensitiveFilePath);
        }
        if (normalized.contains("typical working window")
            || normalized.contains("schedule this background task")
            || normalized.contains("schedule this delegated work"))
            && !directives.contains(&CritiqueDirective::ScheduleForOperatorWindow)
        {
            directives.push(CritiqueDirective::ScheduleForOperatorWindow);
        }
        if normalized.contains("smaller tool-call budget")
            && !directives.contains(&CritiqueDirective::LimitSubagentToolCalls)
        {
            directives.push(CritiqueDirective::LimitSubagentToolCalls);
        }
        if normalized.contains("wall-clock window")
            && !directives.contains(&CritiqueDirective::LimitSubagentWallTime)
        {
            directives.push(CritiqueDirective::LimitSubagentWallTime);
        }
    }
    directives
}

pub(crate) fn resolve(
    advocate: &Argument,
    critic: &Argument,
    risk_tolerance: RiskTolerance,
) -> Resolution {
    let mut resolution = resolve_with_satisfaction_label(advocate, critic, risk_tolerance, None);
    if matches!(risk_tolerance, RiskTolerance::Aggressive)
        && matches!(resolution.decision, Decision::Defer)
    {
        let modifications = recommended_modifications(critic, 2);
        if !modifications.is_empty() {
            let directives = directives_for_modifications(&modifications);
            resolution = Resolution {
                decision: Decision::ProceedWithModifications,
                synthesis: format!(
                    "Proceed with modifications. Keep the action, but incorporate: {}.",
                    modifications.join(" | ")
                ),
                risk_score: resolution.risk_score,
                confidence: resolution.confidence,
                modifications,
                directives,
            };
        }
    }
    resolution
}

pub(crate) fn resolve_with_satisfaction_label(
    advocate: &Argument,
    critic: &Argument,
    risk_tolerance: RiskTolerance,
    satisfaction_label: Option<&str>,
) -> Resolution {
    let advocate_weight: f64 = advocate.points.iter().map(|point| point.weight).sum();
    let critic_weight: f64 = critic.points.iter().map(|point| point.weight).sum();
    let net = advocate_weight - critic_weight;
    let mut proceed_threshold: f64 = match risk_tolerance {
        RiskTolerance::Aggressive => 0.20,
        RiskTolerance::Moderate => 0.45,
        RiskTolerance::Conservative => 0.70,
    };
    let mut defer_band: f64 = match risk_tolerance {
        RiskTolerance::Aggressive => 0.18,
        RiskTolerance::Moderate => 0.25,
        RiskTolerance::Conservative => 0.32,
    };

    match satisfaction_label.unwrap_or_default() {
        "strained" => {
            proceed_threshold -= 0.10;
            defer_band -= 0.08;
        }
        "fragile" => {
            proceed_threshold -= 0.05;
            defer_band -= 0.04;
        }
        _ => {}
    }
    proceed_threshold = proceed_threshold.max(0.05);
    defer_band = defer_band.max(0.08);

    let mut decision = if net >= proceed_threshold {
        Decision::Proceed
    } else if net.abs() <= defer_band {
        Decision::Defer
    } else if net > -0.55 {
        Decision::ProceedWithModifications
    } else {
        Decision::Reject
    };

    let mut modifications = if matches!(decision, Decision::ProceedWithModifications) {
        recommended_modifications(critic, 2)
    } else {
        Vec::new()
    };
    match satisfaction_label.unwrap_or_default() {
        "strained" if matches!(decision, Decision::Defer | Decision::Reject) => {
            let candidate_modifications = recommended_modifications(critic, 2);
            if !candidate_modifications.is_empty() {
                decision = Decision::ProceedWithModifications;
                modifications = candidate_modifications;
            }
        }
        "fragile" if matches!(decision, Decision::Defer) => {
            let candidate_modifications = recommended_modifications(critic, 2);
            if !candidate_modifications.is_empty() {
                decision = Decision::ProceedWithModifications;
                modifications = candidate_modifications;
            }
        }
        _ => {}
    }
    let directives = directives_for_modifications(&modifications);

    let synthesis = match decision {
        Decision::Proceed => format!(
            "Proceed. Strongest supporting considerations: {}. Main residual concern: {}.",
            top_claims(advocate, 2).join(" | "),
            top_claims(critic, 1).join(" | ")
        ),
        Decision::ProceedWithModifications => format!(
            "Proceed with modifications. Keep the action, but incorporate: {}.",
            modifications.join(" | ")
        ),
        Decision::Defer => format!(
            "Defer. Advocate and critic are too close to resolve confidently. Advocate: {}. Critic: {}.",
            top_claims(advocate, 1).join(" | "),
            top_claims(critic, 1).join(" | ")
        ),
        Decision::Reject => format!(
            "Reject. Critic evidence dominates. Strongest objections: {}.",
            top_claims(critic, 2).join(" | ")
        ),
    };

    let total = (advocate_weight + critic_weight).max(0.0001);
    let risk_score = (critic_weight / total).clamp(0.0, 1.0);
    let confidence = ((net.abs() / total) + 0.35).clamp(0.0, 1.0);

    Resolution {
        decision,
        synthesis,
        risk_score,
        confidence,
        modifications,
        directives,
    }
}
