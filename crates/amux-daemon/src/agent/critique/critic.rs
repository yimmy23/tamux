use super::types::{Argument, ArgumentPoint, Role};

fn tool_specific_caution_claim(tool_name: &str, action_summary: &str) -> Option<String> {
    let safe_action_summary = super::sanitize_critique_snippet(action_summary, 96);
    match tool_name {
        "bash_command" | "run_terminal_command" | "execute_managed_command" => Some(format!(
            "Disable network access, enable sandboxing, and downgrade any yolo security level before running {}.",
            safe_action_summary
        )),
        "send_slack_message" | "send_discord_message" | "send_telegram_message"
        | "send_whatsapp_message" => Some(format!(
            "Strip explicit messaging targets and broadcast mentions before sending {}.",
            safe_action_summary
        )),
        "write_file" | "create_file" | "append_to_file" | "replace_in_file"
        | "apply_file_patch" => Some(format!(
            "Narrow the sensitive file path to the minimal basename before applying {}.",
            safe_action_summary
        )),
        "enqueue_task" => Some(format!(
            "Schedule this background task for the operator's typical working window instead of dispatching it immediately: {}.",
            safe_action_summary
        )),
        "spawn_subagent" => Some(format!(
            "Reduce permissions by constraining the child to a smaller tool-call budget and wall-clock window before delegating {}.",
            safe_action_summary
        )),
        "switch_model" => Some(format!(
            "Require explicit operator confirmation before changing the provider or model for {} because it rewrites persisted agent execution policy.",
            safe_action_summary
        )),
        "plugin_api_call" => Some(format!(
            "Require explicit operator confirmation before invoking plugin endpoint {} because plugin API calls can rewrite plugin execution policy or trigger external side effects.",
            safe_action_summary
        )),
        "synthesize_tool" => Some(format!(
            "Require explicit operator confirmation before allowing tool synthesis for {} because synthesizing runtime tools can rewrite runtime tool capability policy.",
            safe_action_summary
        )),
        _ => None,
    }
}

pub(crate) fn build_argument(
    tool_name: &str,
    action_summary: &str,
    reasons: &[String],
    grounded_points: Vec<ArgumentPoint>,
) -> Argument {
    let safe_action_summary = super::sanitize_critique_snippet(action_summary, 96);
    let mut points = Vec::new();

    let risk_weight = if reasons.is_empty() { 0.28 } else { 0.76 };
    points.push(ArgumentPoint {
        claim: format!(
            "'{}' can mutate state or propagate effects beyond a trivial read-only operation.",
            tool_name
        ),
        weight: risk_weight,
        evidence: vec![format!("tool:{}", tool_name)],
    });

    if !reasons.is_empty() {
        points.push(ArgumentPoint {
            claim: "Governance already detected suspicious characteristics that warrant extra scrutiny."
                .to_string(),
            weight: 0.82,
            evidence: reasons
                .iter()
                .take(4)
                .map(|reason| {
                    format!(
                        "governance:{}",
                        super::sanitize_critique_snippet(reason, 140)
                    )
                })
                .collect(),
        });
    }

    points.push(ArgumentPoint {
        claim: format!(
            "Safer alternatives may exist: narrow scope, reduce permissions, or seek operator confirmation before applying {}.",
            safe_action_summary
        ),
        weight: if reasons.is_empty() { 0.32 } else { 0.63 },
        evidence: vec!["heuristic:prefer_narrower_scope".to_string()],
    });

    if let Some(claim) = tool_specific_caution_claim(tool_name, action_summary) {
        points.push(ArgumentPoint {
            claim,
            weight: if reasons.is_empty() { 0.57 } else { 0.74 },
            evidence: vec![format!("tool_specific:{tool_name}:narrower_execution")],
        });
    }

    points.extend(grounded_points);

    Argument {
        role: Role::Critic,
        points,
        overall_confidence: if reasons.is_empty() { 0.34 } else { 0.81 },
    }
}
