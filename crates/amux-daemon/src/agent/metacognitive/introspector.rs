use serde::{Deserialize, Serialize};

use super::types::{CognitiveBias, SelfModel, WorkflowProfile};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentToolOutcome {
    pub tool_name: String,
    pub outcome: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntrospectionInput {
    pub proposed_tool_name: String,
    pub proposed_tool_arguments: String,
    pub normalized_tool_signature: String,
    pub predicted_repeat_count: u32,
    #[serde(default)]
    pub recent_tool_outcomes: Vec<RecentToolOutcome>,
    pub task_retry_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_reasoning: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum InterventionStrength {
    None,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BiasSignal {
    pub bias_name: String,
    pub severity: f64,
    pub strength: InterventionStrength,
    pub rationale: String,
    pub mitigation_prompt: String,
    #[serde(default)]
    pub matched_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_adjustment: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntrospectionOutcome {
    pub tool_name: String,
    pub tool_signature: String,
    pub strength: InterventionStrength,
    #[serde(default)]
    pub signals: Vec<BiasSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_adjustment: Option<f64>,
}

pub fn introspect(self_model: &SelfModel, input: &IntrospectionInput) -> IntrospectionOutcome {
    let mut signals = Vec::new();

    for bias in &self_model.biases {
        match bias.name.as_str() {
            "sunk_cost" => {
                if let Some(signal) = detect_sunk_cost(bias, input) {
                    signals.push(signal);
                }
            }
            "overconfidence" => {
                if let Some(signal) = detect_overconfidence(bias, self_model, input) {
                    signals.push(signal);
                }
            }
            "confirmation" => {
                if let Some(signal) = detect_confirmation_bias(bias, input) {
                    signals.push(signal);
                }
            }
            _ => {}
        }
    }

    let strength = signals
        .iter()
        .map(|signal| signal.strength)
        .max()
        .unwrap_or(InterventionStrength::None);
    let confidence_adjustment = signals
        .iter()
        .filter_map(|signal| signal.confidence_adjustment)
        .min_by(|left, right| left.total_cmp(right));
    let summary = (!signals.is_empty()).then(|| {
        signals
            .iter()
            .map(|signal| format!("{}: {}", signal.bias_name, signal.rationale))
            .collect::<Vec<_>>()
            .join(" | ")
    });

    IntrospectionOutcome {
        tool_name: input.proposed_tool_name.clone(),
        tool_signature: input.normalized_tool_signature.clone(),
        strength,
        signals,
        summary,
        confidence_adjustment,
    }
}

fn detect_sunk_cost(bias: &CognitiveBias, input: &IntrospectionInput) -> Option<BiasSignal> {
    let same_tool_failures = recent_same_tool_failures(input);
    let repeat_limit = bias.trigger_pattern.max_revisions.max(3);
    let known_tool_match = bias.trigger_pattern.tool_sequence.is_empty()
        || bias
            .trigger_pattern
            .tool_sequence
            .iter()
            .any(|tool| tool == &input.proposed_tool_name);
    let repeated_loop = input.predicted_repeat_count >= repeat_limit;
    let failure_loop = same_tool_failures >= repeat_limit as usize;
    let retry_loop = input.task_retry_count >= repeat_limit;

    if !(repeated_loop || failure_loop || (known_tool_match && retry_loop)) {
        return None;
    }

    let mut matched_on = Vec::new();
    if known_tool_match {
        matched_on.push(format!("tool:{}", input.proposed_tool_name));
    }
    if repeated_loop {
        matched_on.push(format!(
            "predicted_repeat_count:{}",
            input.predicted_repeat_count
        ));
    }
    if failure_loop {
        matched_on.push(format!("recent_same_tool_failures:{}", same_tool_failures));
    }
    if retry_loop {
        matched_on.push(format!("task_retry_count:{}", input.task_retry_count));
    }

    let rationale = format!(
        "Repeated tool usage suggests a sunk-cost loop (repeat_count={}, same_tool_failures={}, task_retries={}).",
        input.predicted_repeat_count, same_tool_failures, input.task_retry_count
    );

    Some(BiasSignal {
        bias_name: bias.name.clone(),
        severity: bias.severity,
        strength: InterventionStrength::Block,
        rationale,
        mitigation_prompt: bias.mitigation_prompt.clone(),
        matched_on,
        confidence_adjustment: None,
    })
}

fn detect_confirmation_bias(
    bias: &CognitiveBias,
    input: &IntrospectionInput,
) -> Option<BiasSignal> {
    let investigative_tool = bias
        .trigger_pattern
        .tool_sequence
        .iter()
        .any(|tool| tool == &input.proposed_tool_name);
    if !investigative_tool {
        return None;
    }

    let reasoning = input.decision_reasoning.as_deref().unwrap_or("");
    let confirmatory_reasoning = sounds_confirmatory(reasoning);

    let recent = input
        .recent_tool_outcomes
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>();
    if recent.len() < 3 {
        return None;
    }

    let same_family = recent.iter().all(|outcome| {
        bias.trigger_pattern
            .tool_sequence
            .iter()
            .any(|tool| tool == &outcome.tool_name)
    });
    let all_success = recent
        .iter()
        .all(|outcome| outcome.outcome.eq_ignore_ascii_case("success"));
    let no_executive_check = recent.iter().all(|outcome| {
        !matches!(
            outcome.tool_name.as_str(),
            "bash_command" | "run_terminal_command" | "cargo" | "execute_managed_command"
        )
    });
    let repeated_probe = input.predicted_repeat_count >= 2
        || recent
            .iter()
            .filter(|outcome| outcome.tool_name == input.proposed_tool_name)
            .count()
            >= 2;
    let positive_evidence_hits = recent
        .iter()
        .filter(|outcome| contains_positive_evidence(&outcome.summary))
        .count();
    let selective_validation =
        confirmatory_reasoning || (input.task_retry_count > 0 && positive_evidence_hits >= 1);

    if !(same_family && all_success && no_executive_check && repeated_probe && selective_validation)
    {
        return None;
    }

    Some(BiasSignal {
        bias_name: bias.name.clone(),
        severity: bias.severity,
        strength: InterventionStrength::Warn,
        rationale: format!(
            "Recent activity is clustered in evidence-gathering tools without a disconfirming execution check before repeating `{}`.",
            input.proposed_tool_name
        ),
        mitigation_prompt: bias.mitigation_prompt.clone(),
        matched_on: vec![
            format!("tool:{}", input.proposed_tool_name),
            format!("recent_same_family:{}", recent.len()),
            format!("positive_evidence_hits:{}", positive_evidence_hits),
            if confirmatory_reasoning {
                "confirmatory_reasoning".to_string()
            } else {
                format!("retry_pressure:{}", input.task_retry_count)
            },
            "no_disconfirming_execution_check".to_string(),
        ],
        confidence_adjustment: Some(-(bias.severity * 0.08).clamp(0.02, 0.08)),
    })
}

fn detect_overconfidence(
    bias: &CognitiveBias,
    self_model: &SelfModel,
    input: &IntrospectionInput,
) -> Option<BiasSignal> {
    let reasoning = input.decision_reasoning.as_deref().unwrap_or("");
    if !sounds_overconfident(reasoning) {
        return None;
    }

    let recent_success_rate = recent_success_rate(input);
    let low_success_recently = recent_success_rate < 0.5;
    let low_success_profile = workflow_profile_for_tool(self_model, &input.proposed_tool_name)
        .is_some_and(|profile| profile.avg_success_rate < 0.65);
    let high_retry_pressure = input.task_retry_count >= 2;

    if !(low_success_recently || low_success_profile || high_retry_pressure) {
        return None;
    }

    let mut matched_on = vec!["confident_language".to_string()];
    if low_success_recently {
        matched_on.push(format!("recent_success_rate:{recent_success_rate:.2}"));
    }
    if let Some(profile) = workflow_profile_for_tool(self_model, &input.proposed_tool_name) {
        matched_on.push(format!(
            "workflow_profile:{}:{:.2}",
            profile.name, profile.avg_success_rate
        ));
    }
    if high_retry_pressure {
        matched_on.push(format!("task_retry_count:{}", input.task_retry_count));
    }

    let rationale = if let Some(profile) =
        workflow_profile_for_tool(self_model, &input.proposed_tool_name)
    {
        format!(
            "Decision reasoning sounds overly certain while `{}` historically succeeds only {:.0}% of the time in workflow `{}`.",
            input.proposed_tool_name,
            profile.avg_success_rate * 100.0,
            profile.name
        )
    } else {
        format!(
            "Decision reasoning sounds overly certain despite recent tool success rate of only {:.0}%.",
            recent_success_rate * 100.0
        )
    };

    Some(BiasSignal {
        bias_name: bias.name.clone(),
        severity: bias.severity,
        strength: if recent_success_rate < 0.25 && input.task_retry_count >= 3 {
            InterventionStrength::Block
        } else {
            InterventionStrength::Warn
        },
        rationale,
        mitigation_prompt: bias.mitigation_prompt.clone(),
        matched_on,
        confidence_adjustment: Some(-(bias.severity * 0.1).clamp(0.03, 0.15)),
    })
}

fn recent_same_tool_failures(input: &IntrospectionInput) -> usize {
    input
        .recent_tool_outcomes
        .iter()
        .rev()
        .take(6)
        .filter(|outcome| {
            outcome.tool_name == input.proposed_tool_name
                && outcome.outcome.eq_ignore_ascii_case("failure")
        })
        .count()
}

fn recent_success_rate(input: &IntrospectionInput) -> f64 {
    let recent = input
        .recent_tool_outcomes
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>();
    if recent.is_empty() {
        return 1.0;
    }

    let successes = recent
        .iter()
        .filter(|outcome| outcome.outcome.eq_ignore_ascii_case("success"))
        .count();
    successes as f64 / recent.len() as f64
}

fn workflow_profile_for_tool<'a>(
    self_model: &'a SelfModel,
    tool_name: &str,
) -> Option<&'a WorkflowProfile> {
    self_model
        .workflow_profiles
        .iter()
        .filter(|profile| profile.typical_tools.iter().any(|tool| tool == tool_name))
        .min_by(|left, right| left.avg_success_rate.total_cmp(&right.avg_success_rate))
}

fn sounds_overconfident(reasoning: &str) -> bool {
    let normalized = reasoning.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    [
        "definitely",
        "certain",
        "obviously",
        "clearly",
        "guaranteed",
        "this will work",
        "should work",
        "the fix is",
        "the answer is",
        "i'll just",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn sounds_confirmatory(reasoning: &str) -> bool {
    let normalized = reasoning.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    [
        "confirm",
        "verify",
        "looks right",
        "seems right",
        "matches",
        "as expected",
        "probably correct",
        "should be enough",
        "just check",
        "double-check",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn contains_positive_evidence(summary: &str) -> bool {
    let normalized = summary.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    [
        "found", "match", "matches", "present", "exists", "contains", "resolved", "restored",
        "loaded", "verified",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}
