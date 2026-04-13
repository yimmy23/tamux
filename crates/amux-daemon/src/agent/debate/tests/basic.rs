use crate::agent::debate::protocol::{
    advance_round, assign_roles, create_debate_session, finalize_verdict, validate_argument,
};
use crate::agent::debate::types::{Argument, DebateStatus, RoleKind};
use crate::agent::handoff::divergent::Framing;

fn sample_framings() -> Vec<Framing> {
    vec![
        Framing {
            label: "analytical-lens".to_string(),
            system_prompt_override: "Analyze formally".to_string(),
            task_id: None,
            contribution_id: None,
        },
        Framing {
            label: "pragmatic-lens".to_string(),
            system_prompt_override: "Be pragmatic".to_string(),
            task_id: None,
            contribution_id: None,
        },
        Framing {
            label: "synthesis-lens".to_string(),
            system_prompt_override: "Synthesize".to_string(),
            task_id: None,
            contribution_id: None,
        },
    ]
}

#[test]
fn create_debate_session_initializes_round_one() {
    let session = create_debate_session(
        "cache strategy".to_string(),
        sample_framings(),
        3,
        true,
        Some("thread-1".to_string()),
        None,
    )
    .expect("create session");
    assert_eq!(session.current_round, 1);
    assert_eq!(session.status, DebateStatus::InProgress);
    assert_eq!(session.roles.len(), 3);
    assert_eq!(session.roles[0].role, RoleKind::Proponent);
}

#[test]
fn assign_roles_rotates_proponent_and_skeptic_after_round_two() {
    let framings = sample_framings();
    let round_one = assign_roles(&framings, 1, true);
    let round_three = assign_roles(&framings, 3, true);
    assert_eq!(round_one[0].agent_id, "analytical-lens");
    assert_eq!(round_one[1].agent_id, "pragmatic-lens");
    assert_eq!(round_three[0].agent_id, "pragmatic-lens");
    assert_eq!(round_three[1].agent_id, "analytical-lens");
}

#[test]
fn validate_argument_requires_evidence_and_known_response_target() {
    let bad = Argument {
        id: "a1".to_string(),
        round: 2,
        role: RoleKind::Skeptic,
        agent_id: "skeptic".to_string(),
        content: "counterargument".to_string(),
        evidence_refs: vec![],
        responds_to: Some("missing".to_string()),
        timestamp_ms: 1,
    };
    assert!(validate_argument(&bad, 1, &[]).is_err());

    let good = Argument {
        evidence_refs: vec!["file:Cargo.toml".to_string()],
        responds_to: Some("a0".to_string()),
        ..bad
    };
    assert!(validate_argument(&good, 1, &["a0".to_string()]).is_ok());
}

#[test]
fn advance_round_and_finalize_verdict_progress_session() {
    let mut session = create_debate_session(
        "debate topic".to_string(),
        sample_framings(),
        3,
        true,
        None,
        None,
    )
    .expect("create session");
    advance_round(&mut session, true).expect("advance round");
    assert_eq!(session.current_round, 2);

    finalize_verdict(
        &mut session,
        vec!["agree on phased rollout".to_string()],
        vec!["observability budget".to_string()],
        "Run a small canary first".to_string(),
        0.8,
    )
    .expect("finalize verdict");
    assert_eq!(session.status, DebateStatus::Completed);
    assert!(session.verdict.is_some());
}
