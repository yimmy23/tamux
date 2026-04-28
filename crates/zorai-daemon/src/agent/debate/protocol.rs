use anyhow::Result;

use super::types::{
    Argument, DebateRole, DebateRoundRequest, DebateSession, DebateStatus, DebateVerdict, RoleKind,
};
use crate::agent::handoff::divergent::Framing;

pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn assign_roles(
    framings: &[Framing],
    round: u8,
    role_rotation: bool,
) -> Vec<DebateRole> {
    let mut roles = Vec::new();
    if framings.is_empty() {
        return roles;
    }

    let proponent_idx = if role_rotation && round >= 3 && framings.len() >= 2 {
        1
    } else {
        0
    };
    let skeptic_idx = if framings.len() >= 2 {
        if role_rotation && round >= 3 {
            0
        } else {
            1
        }
    } else {
        0
    };
    let synthesizer_idx = if framings.len() >= 3 {
        2
    } else {
        framings.len() - 1
    };

    roles.push(DebateRole {
        role: RoleKind::Proponent,
        agent_id: framings[proponent_idx].label.clone(),
        system_prompt_override: Some(format!(
            "You are the proponent for round {}. Defend the strongest actionable position with evidence.",
            round
        )),
    });
    roles.push(DebateRole {
        role: RoleKind::Skeptic,
        agent_id: framings[skeptic_idx].label.clone(),
        system_prompt_override: Some(format!(
            "You are the skeptic for round {}. Challenge the proposed position with evidence-backed counterarguments.",
            round
        )),
    });
    roles.push(DebateRole {
        role: RoleKind::Synthesizer,
        agent_id: framings[synthesizer_idx].label.clone(),
        system_prompt_override: Some(format!(
            "You are the synthesizer for round {}. Identify convergence and unresolved tensions without forcing consensus.",
            round
        )),
    });
    roles
}

pub(crate) fn build_debate_round_requests(session: &DebateSession) -> Vec<DebateRoundRequest> {
    let prior_argument_ids = session
        .arguments
        .iter()
        .map(|argument| argument.id.clone())
        .collect::<Vec<_>>();

    session
        .roles
        .iter()
        .map(|role| {
            let framing = session
                .framings
                .iter()
                .find(|framing| framing.label == role.agent_id);
            let prompt = format!(
                "Debate topic: {}\nRound: {}\nRole: {}\n{}\nPrior arguments in session: {}.",
                session.topic,
                session.current_round,
                role.role.as_str(),
                role.system_prompt_override
                    .as_deref()
                    .unwrap_or("Continue the debate with evidence-grounded reasoning."),
                prior_argument_ids.len()
            );

            DebateRoundRequest {
                session_id: session.id.clone(),
                round: session.current_round,
                role: role.role,
                agent_id: role.agent_id.clone(),
                topic: session.topic.clone(),
                prompt,
                framing_task_id: framing.and_then(|value| value.task_id.clone()),
                framing_contribution_id: framing.and_then(|value| value.contribution_id.clone()),
                prior_argument_ids: prior_argument_ids.clone(),
            }
        })
        .collect()
}

pub(crate) fn validate_argument(
    argument: &Argument,
    min_evidence_refs: usize,
    known_argument_ids: &[String],
) -> Result<()> {
    if argument.content.trim().is_empty() {
        anyhow::bail!("debate argument content cannot be empty");
    }
    if argument.evidence_refs.len() < min_evidence_refs {
        anyhow::bail!(
            "debate argument requires at least {} evidence reference(s)",
            min_evidence_refs
        );
    }
    if let Some(ref responds_to) = argument.responds_to {
        if !known_argument_ids.iter().any(|id| id == responds_to) {
            anyhow::bail!("debate argument responds_to references unknown argument {responds_to}");
        }
    }
    Ok(())
}

pub(crate) fn create_debate_session(
    topic: String,
    framings: Vec<Framing>,
    max_rounds: u8,
    role_rotation: bool,
    thread_id: Option<String>,
    goal_run_id: Option<String>,
) -> Result<DebateSession> {
    if framings.len() < 2 {
        anyhow::bail!("debate session requires at least 2 framings");
    }
    let current_round = 1;
    Ok(DebateSession {
        id: format!("debate_{}", uuid::Uuid::new_v4()),
        topic,
        framings: framings.clone(),
        max_rounds: max_rounds.max(1),
        current_round,
        roles: assign_roles(&framings, current_round, role_rotation),
        arguments: Vec::new(),
        verdict: None,
        status: DebateStatus::InProgress,
        created_at_ms: now_millis(),
        completed_at_ms: None,
        completion_reason: None,
        thread_id,
        goal_run_id,
    })
}

pub(crate) fn advance_round(session: &mut DebateSession, role_rotation: bool) -> Result<()> {
    if session.status != DebateStatus::InProgress {
        anyhow::bail!("cannot advance debate round when status is not in_progress");
    }
    if session.current_round >= session.max_rounds {
        anyhow::bail!("cannot advance beyond max_rounds");
    }
    session.current_round += 1;
    session.roles = assign_roles(&session.framings, session.current_round, role_rotation);
    Ok(())
}

pub(crate) fn finalize_verdict(
    session: &mut DebateSession,
    consensus_points: Vec<String>,
    unresolved_tensions: Vec<String>,
    recommended_action: String,
    confidence: f64,
    completion_reason: impl Into<String>,
) -> Result<()> {
    let synthesizer_agent = session
        .roles
        .iter()
        .find(|role| role.role == RoleKind::Synthesizer)
        .map(|role| role.agent_id.clone())
        .unwrap_or_else(|| "synthesizer".to_string());
    session.verdict = Some(DebateVerdict {
        consensus_points,
        unresolved_tensions,
        recommended_action,
        confidence: confidence.clamp(0.0, 1.0),
        synthesizer_agent,
    });
    session.status = DebateStatus::Completed;
    session.completed_at_ms = Some(now_millis());
    session.completion_reason = Some(completion_reason.into());
    Ok(())
}
