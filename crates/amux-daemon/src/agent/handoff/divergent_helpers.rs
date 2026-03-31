use super::{DivergentSession, Framing};
use crate::agent::collaboration::Disagreement;

pub(super) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) fn generate_framing_prompts(problem: &str) -> Vec<Framing> {
    vec![
        Framing {
            label: "analytical-lens".to_string(),
            system_prompt_override: format!(
                "Approach this problem analytically. Focus on correctness, edge cases, \
                 and formal reasoning. Identify what could go wrong.\n\nProblem: {}",
                problem
            ),
            task_id: None,
            contribution_id: None,
        },
        Framing {
            label: "pragmatic-lens".to_string(),
            system_prompt_override: format!(
                "Approach this problem pragmatically. Focus on simplicity, speed of \
                 delivery, and practical tradeoffs. Identify what gets results fastest.\n\n\
                 Problem: {}",
                problem
            ),
            task_id: None,
            contribution_id: None,
        },
    ]
}

pub(super) fn format_tensions(disagreements: &[Disagreement], framings: &[Framing]) -> String {
    if disagreements.is_empty() {
        return "No significant disagreements detected between framings.".to_string();
    }

    let mut output = String::new();
    for disagreement in disagreements {
        output.push_str(&format!("### {}\n\n", disagreement.topic));

        for (idx, position) in disagreement.positions.iter().enumerate() {
            let fallback = format!("Position {}", (b'A' + idx as u8) as char);
            let label = if idx < framings.len() {
                &framings[idx].label
            } else {
                &fallback
            };
            output.push_str(&format!("**{}:** {}\n", label, position));
        }

        if !disagreement.votes.is_empty() {
            let vote_summary: Vec<String> = disagreement
                .votes
                .iter()
                .map(|vote| {
                    format!(
                        "{}: {} (weight {:.1})",
                        vote.task_id, vote.position, vote.weight
                    )
                })
                .collect();
            output.push_str(&format!("\nEvidence: {}\n", vote_summary.join("; ")));
        } else {
            output.push_str(&format!(
                "\nEvidence: confidence gap {:.2}\n",
                disagreement.confidence_gap
            ));
        }
        output.push('\n');
    }
    output
}

pub(super) fn format_mediator_prompt(session: &DivergentSession, tensions: &str) -> String {
    let framing_descriptions: Vec<String> = session
        .framings
        .iter()
        .map(|framing| {
            format!(
                "- **{}**: {}",
                framing.label, framing.system_prompt_override
            )
        })
        .collect();

    format!(
        "You are mediating between {} different perspectives on a problem.\n\n\
         ## Problem\n{}\n\n\
         ## Framings\n{}\n\n\
         ## Tensions Identified\n{}\n\n\
         ## Your Task\n\
         Synthesize these tensions into a recommendation that:\n\
         1. Acknowledges the valid concerns from each perspective\n\
         2. Identifies the key tradeoffs (do NOT force consensus)\n\
         3. Recommends a path forward with explicit acknowledgment of what is sacrificed\n\
         4. Notes which concerns remain unresolved\n\n\
         Do NOT pick a \"winner.\" Surface the tradeoffs clearly so the operator can make an informed decision.",
        session.framings.len(),
        session.problem_statement,
        framing_descriptions.join("\n"),
        tensions
    )
}
