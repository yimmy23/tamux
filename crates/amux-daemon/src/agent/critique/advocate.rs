use super::types::{Argument, ArgumentPoint, Role};

pub(crate) fn build_argument(
    tool_name: &str,
    action_summary: &str,
    reasons: &[String],
    grounded_points: Vec<ArgumentPoint>,
) -> Argument {
    let safe_action_summary = super::sanitize_critique_snippet(action_summary, 120);
    let mut points = Vec::new();
    points.push(ArgumentPoint {
        claim: format!(
            "Executing '{}' advances the requested workflow without forcing additional operator latency.",
            tool_name
        ),
        weight: 0.62,
        evidence: vec![format!("tool:{}", tool_name)],
    });
    points.push(ArgumentPoint {
        claim: format!(
            "The action summary is concrete enough to evaluate: {}",
            safe_action_summary
        ),
        weight: 0.44,
        evidence: vec!["input:action_summary".to_string()],
    });
    if !reasons.is_empty() {
        points.push(ArgumentPoint {
            claim: "Known risks appear bounded or can be mitigated with explicit caution."
                .to_string(),
            weight: 0.36,
            evidence: reasons
                .iter()
                .take(3)
                .map(|reason| {
                    format!(
                        "risk_reason:{}",
                        super::sanitize_critique_snippet(reason, 140)
                    )
                })
                .collect(),
        });
    }
    points.extend(grounded_points);

    Argument {
        role: Role::Advocate,
        points,
        overall_confidence: if reasons.is_empty() { 0.74 } else { 0.58 },
    }
}
