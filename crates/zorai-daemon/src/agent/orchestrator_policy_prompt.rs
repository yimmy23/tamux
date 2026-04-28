use super::*;

const POLICY_PROMPT_MAX_TOOL_OUTCOMES: usize = 4;
const POLICY_PROMPT_MAX_FIELD_CHARS: usize = 220;
const POLICY_PROMPT_MAX_TOOL_SUMMARY_CHARS: usize = 160;

fn normalized_optional_text(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| normalize_policy_prompt_text(value, POLICY_PROMPT_MAX_FIELD_CHARS))
}

fn normalize_policy_prompt_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.is_empty() {
        return String::new();
    }

    let mut normalized = String::new();
    for (index, ch) in collapsed.chars().enumerate() {
        if index >= max_chars {
            normalized.push_str("...");
            break;
        }
        normalized.push(ch);
    }

    normalized
}

fn format_policy_prompt_section(title: &str, value: Option<&str>) -> String {
    let content = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    format!("## {title}\n{content}\n")
}

pub(crate) fn build_policy_eval_prompt(context: &PolicyEvaluationContext) -> String {
    let mut prompt = String::from(
        "You are evaluating whether the zorai orchestrator should continue, pivot, escalate, or halt_retries.\n\
         Return strict JSON only with this shape:\n\
         {\"action\":\"continue|pivot|escalate|halt_retries\",\"reason\":\"...\",\"strategy_hint\":null}\n\
         Requirements:\n\
         - Use `continue` when evidence is weak or mixed.\n\
         - Keep `reason` short and concrete.\n\
         - Keep `strategy_hint` short and only use it for pivot.\n\
         - Do not return `retry_guard`; runtime owns any retry guard binding.\n\
         - Do not invent missing context.\n\n",
    );

    let trigger = &context.trigger;
    let goal_run_id = trigger.goal_run_id.as_deref().unwrap_or("none");
    prompt.push_str(&format!(
        "## Trigger context\nthread_id: {}\ngoal_run_id: {}\nrepeated_approach: {}\nawareness_stuck: {}\nself_assessment.should_pivot: {}\nself_assessment.should_escalate: {}\n\n",
        trigger.thread_id,
        goal_run_id,
        trigger.repeated_approach,
        trigger.awareness_stuck,
        trigger.self_assessment.should_pivot,
        trigger.self_assessment.should_escalate,
    ));

    let tool_outcomes = if context.recent_tool_outcomes.is_empty() {
        "- none".to_string()
    } else {
        let rendered = context
            .recent_tool_outcomes
            .iter()
            .take(POLICY_PROMPT_MAX_TOOL_OUTCOMES)
            .map(|outcome| {
                format!(
                    "- {} => {}: {}",
                    normalize_policy_prompt_text(&outcome.tool_name, 40),
                    normalize_policy_prompt_text(&outcome.outcome, 20),
                    normalize_policy_prompt_text(
                        &outcome.summary,
                        POLICY_PROMPT_MAX_TOOL_SUMMARY_CHARS,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let omitted_count = context
            .recent_tool_outcomes
            .len()
            .saturating_sub(POLICY_PROMPT_MAX_TOOL_OUTCOMES);
        let mut lines = rendered;
        if omitted_count > 0 {
            lines.push(format!(
                "- ... {omitted_count} additional tool outcomes omitted"
            ));
        }
        lines.join("\n")
    };
    prompt.push_str(&format!("## Recent tool outcomes\n{tool_outcomes}\n\n"));

    prompt.push_str(&format_policy_prompt_section(
        "Awareness summary",
        normalized_optional_text(&context.awareness_summary).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Continuity summary",
        normalized_optional_text(&context.continuity_summary).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Counter-who context",
        normalized_optional_text(&context.counter_who_context).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Ruled-out approaches",
        normalized_optional_text(&context.negative_constraints_context).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Self-assessment summary",
        normalized_optional_text(&context.self_assessment_summary).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Thread context",
        normalized_optional_text(&context.thread_context).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Recent policy decision summary",
        normalized_optional_text(&context.recent_decision_summary).as_deref(),
    ));

    prompt
}

pub(crate) fn continue_policy_decision(reason: &str) -> PolicyDecision {
    PolicyDecision {
        action: PolicyAction::Continue,
        reason: reason.to_string(),
        strategy_hint: None,
        retry_guard: None,
    }
}

pub(crate) fn normalize_policy_eval_decision(decision: Option<PolicyDecision>) -> PolicyDecision {
    match decision {
        Some(decision) => match validate_policy_decision(&decision) {
            Ok(validated) => validated,
            Err(_) => continue_policy_decision(
                "Policy evaluation returned an invalid decision; continuing current execution.",
            ),
        },
        None => {
            continue_policy_decision("Policy evaluation unavailable; continuing current execution.")
        }
    }
}

pub(crate) fn runtime_owns_policy_retry_guard(
    decision: PolicyDecision,
    current_retry_guard: Option<&str>,
) -> PolicyDecision {
    match decision.action {
        PolicyAction::Continue | PolicyAction::Escalate => PolicyDecision {
            retry_guard: None,
            ..decision
        },
        PolicyAction::Pivot => PolicyDecision {
            retry_guard: None,
            ..decision
        },
        PolicyAction::HaltRetries => {
            let Some(current_retry_guard) = current_retry_guard
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return continue_policy_decision(
                    "Policy evaluation requested halt_retries without a live retry guard; continuing current execution.",
                );
            };

            PolicyDecision {
                retry_guard: Some(current_retry_guard.to_string()),
                ..decision
            }
        }
    }
}
