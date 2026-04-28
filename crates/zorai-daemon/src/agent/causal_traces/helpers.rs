#[derive(Debug, Default)]
pub(crate) struct FamilyOutcomeSummary {
    pub failure_count: u32,
    pub near_miss_count: u32,
    pub reasons: Vec<String>,
    pub recoveries: Vec<String>,
}

impl FamilyOutcomeSummary {
    pub fn record(&mut self, summary: OutcomeSummary) {
        if summary.is_near_miss {
            self.near_miss_count = self.near_miss_count.saturating_add(1);
        } else {
            self.failure_count = self.failure_count.saturating_add(1);
        }
        if self.reasons.len() < 2 {
            self.reasons.push(summary.reason);
        }
        if let Some(recovery) = summary.recovery {
            if self.recoveries.len() < 2 {
                self.recoveries.push(recovery);
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct OutcomeSummary {
    pub reason: String,
    pub recovery: Option<String>,
    pub is_near_miss: bool,
}

pub(crate) fn estimated_success_probability(
    prior_successes: usize,
    prior_failures: usize,
    is_error: bool,
) -> f64 {
    let total = prior_successes + prior_failures;
    if total == 0 {
        return if is_error { 0.35 } else { 0.65 };
    }
    let historical = prior_successes as f64 / total as f64;
    if is_error {
        (historical * 0.8).clamp(0.0, 1.0)
    } else {
        historical.clamp(0.0, 1.0)
    }
}

pub(crate) fn estimate_plan_success(step_count: usize, command_steps: usize) -> f64 {
    let complexity_penalty = ((step_count.saturating_sub(2)) as f64 * 0.08).min(0.32);
    let command_penalty = (command_steps as f64 * 0.05).min(0.2);
    (0.82 - complexity_penalty - command_penalty).clamp(0.2, 0.9)
}

pub(crate) fn command_family_from_tool_args(arguments_json: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(arguments_json).ok()?;
    let command = parsed.get("command")?.as_str()?;
    Some(command_family(command))
}

pub(crate) fn command_family(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    let tokens = trimmed.split_whitespace().take(2).collect::<Vec<_>>();
    let family = tokens.join(" ");
    family
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn pattern_family_from_factor(
    factor: &crate::agent::learning::traces::CausalFactor,
) -> Option<String> {
    if factor.factor_type != crate::agent::learning::traces::FactorType::PatternMatch {
        return None;
    }
    if let Some(family) = factor
        .description
        .strip_prefix("command family: ")
        .map(str::to_string)
    {
        return Some(family);
    }
    if let Some(signature) = factor.description.strip_prefix("upstream signature: ") {
        return Some(command_family(signature));
    }
    factor
        .description
        .strip_prefix("upstream class: ")
        .map(command_family)
}

pub(crate) fn summarize_outcome(
    outcome: crate::agent::learning::traces::CausalTraceOutcome,
) -> Option<OutcomeSummary> {
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            Some(OutcomeSummary {
                reason,
                recovery: None,
                is_near_miss: false,
            })
        }
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong,
            how_recovered,
        } => Some(OutcomeSummary {
            reason: what_went_wrong,
            recovery: Some(how_recovered),
            is_near_miss: true,
        }),
        _ => None,
    }
}
